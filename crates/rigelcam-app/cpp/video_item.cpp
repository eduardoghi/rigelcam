#include <QGuiApplication>
#include <QIcon>
#include "video_item.h"

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <GLES2/gl2ext.h>
#include <QtGui/QMatrix4x4>
#include <QtGui/QOpenGLContext>
#include <QtGui/QOpenGLFunctions>
#include <QtGui/QSurfaceFormat>
#include <QtQml/qqml.h>
#include <QtQuick/QSGRenderNode>
#include <sys/stat.h>

#include <cstdio>
#include <map>

namespace {

const char *kVertex =
    "#version 100\n"
    "attribute vec2 pos; attribute vec2 uv; uniform mat4 mvp; varying vec2 v;\n"
    "void main() { v = uv; gl_Position = mvp * vec4(pos, 0.0, 1.0); }\n";
const char *kFragment =
    "#version 100\n"
    "#extension GL_OES_EGL_image_external : require\n"
    "precision mediump float; varying vec2 v; uniform samplerExternalOES tex;\n"
    "void main() { gl_FragColor = texture2D(tex, v); }\n";

struct Program {
    GLuint id = 0;
    GLint mvp = -1, tex = -1, pos = -1, uv = -1;
    GLuint vbo = 0;
};

GLuint compile(QOpenGLFunctions *f, GLenum type, const char *source) {
    GLuint shader = f->glCreateShader(type);
    f->glShaderSource(shader, 1, &source, nullptr);
    f->glCompileShader(shader);
    GLint ok = 0;
    f->glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[512] = {};
        f->glGetShaderInfoLog(shader, sizeof log, nullptr, log);
        fprintf(stderr, "shader error (%s)\n", log);
    }
    return shader;
}

// Shared by all items, which draw on the same thread and in the same context.
Program &program(QOpenGLFunctions *f) {
    static Program p;
    if (!p.id) {
        p.id = f->glCreateProgram();
        f->glAttachShader(p.id, compile(f, GL_VERTEX_SHADER, kVertex));
        f->glAttachShader(p.id, compile(f, GL_FRAGMENT_SHADER, kFragment));
        f->glLinkProgram(p.id);
        p.mvp = f->glGetUniformLocation(p.id, "mvp");
        p.tex = f->glGetUniformLocation(p.id, "tex");
        p.pos = f->glGetAttribLocation(p.id, "pos");
        p.uv = f->glGetAttribLocation(p.id, "uv");
        f->glGenBuffers(1, &p.vbo);
    }
    return p;
}

constexpr size_t kMaxImportedSurfaces = 64;

struct Imported {
    EGLImageKHR image = EGL_NO_IMAGE_KHR;
    GLuint texture = 0;
};

void release(RigelFrame &frame) {
    if (frame.release) frame.release(frame.owner);
    frame = RigelFrame{};
}

class VideoNode : public QSGRenderNode {
public:
    ~VideoNode() override {
        release(m_next);
        release(m_current);
        release(m_previous);
    }

    void setFrame(const RigelFrame &frame, const QRectF &rect) {
        m_rect = rect;
        if (frame.release) {
            release(m_next);
            m_next = frame;
        }
    }

    StateFlags changedStates() const override { return BlendState | DepthState | ScissorState; }
    RenderingFlags flags() const override { return BoundedRectRendering | OpaqueRendering; }
    QRectF rect() const override { return m_rect; }

    void releaseResources() override { clearCache(); }

    void render(const RenderState *state) override {
        auto *f = QOpenGLContext::currentContext()->functions();
        // A frame goes back to the decoder only after a newer one was drawn, since the GPU may
        // still be sampling it.
        if (m_next.release) {
            release(m_previous);
            m_previous = m_current;
            m_current = m_next;
            m_next = RigelFrame{};
        }
        if (!m_current.release) return;
        GLuint texture = textureFor(f, m_current);
        if (!texture) return;

        Program &p = program(f);
        const float w = float(m_rect.width()), h = float(m_rect.height());
        const float quad[] = {0, 0, 0, 0, w, 0, 1, 0, 0, h, 0, 1, w, h, 1, 1};
        const QMatrix4x4 mvp = *state->projectionMatrix() * *matrix();

        f->glDisable(GL_BLEND);
        f->glDisable(GL_DEPTH_TEST);
        // The scene graph leaves the scissor of the last clipped item on, so the first video after
        // a clipped sidebar would be cut out entirely.
        if (state->scissorEnabled()) {
            const QRect scissor = state->scissorRect();
            f->glEnable(GL_SCISSOR_TEST);
            f->glScissor(scissor.x(), scissor.y(), scissor.width(), scissor.height());
        } else {
            f->glDisable(GL_SCISSOR_TEST);
        }
        f->glUseProgram(p.id);
        f->glUniformMatrix4fv(p.mvp, 1, GL_FALSE, mvp.constData());
        f->glUniform1i(p.tex, 0);
        f->glActiveTexture(GL_TEXTURE0);
        f->glBindTexture(GL_TEXTURE_EXTERNAL_OES, texture);
        f->glBindBuffer(GL_ARRAY_BUFFER, p.vbo);
        f->glBufferData(GL_ARRAY_BUFFER, sizeof quad, quad, GL_STREAM_DRAW);
        f->glEnableVertexAttribArray(p.pos);
        f->glEnableVertexAttribArray(p.uv);
        f->glVertexAttribPointer(p.pos, 2, GL_FLOAT, GL_FALSE, 16, nullptr);
        f->glVertexAttribPointer(p.uv, 2, GL_FLOAT, GL_FALSE, 16, reinterpret_cast<void *>(8));
        f->glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        f->glDisableVertexAttribArray(p.pos);
        f->glDisableVertexAttribArray(p.uv);
        f->glBindTexture(GL_TEXTURE_EXTERNAL_OES, 0);
    }

private:
    void clearCache() {
        auto *context = QOpenGLContext::currentContext();
        if (!context) return;
        static auto destroyImage = reinterpret_cast<PFNEGLDESTROYIMAGEKHRPROC>(eglGetProcAddress("eglDestroyImageKHR"));
        for (auto &[inode, imported] : m_cache) {
            context->functions()->glDeleteTextures(1, &imported.texture);
            destroyImage(eglGetCurrentDisplay(), imported.image);
        }
        m_cache.clear();
    }

    // The decoder recycles a fixed set of surfaces, so each one is imported only once.
    GLuint textureFor(QOpenGLFunctions *f, const RigelFrame &fr) {
        if (fr.generation != m_generation) {
            clearCache();
            m_generation = fr.generation;
        }
        struct stat st {};
        if (fstat(fr.fds[0], &st) != 0) return 0;
        auto it = m_cache.find(st.st_ino);
        if (it != m_cache.end()) return it->second.texture;

        // More surfaces than any decoder pool uses, so a bad stream can't grow this without limit.
        if (m_cache.size() >= kMaxImportedSurfaces) clearCache();

        static auto createImage = reinterpret_cast<PFNEGLCREATEIMAGEKHRPROC>(eglGetProcAddress("eglCreateImageKHR"));
        static auto targetTexture = reinterpret_cast<PFNGLEGLIMAGETARGETTEXTURE2DOESPROC>(eglGetProcAddress("glEGLImageTargetTexture2DOES"));
        EGLint a[64];
        int n = 0;
        auto add = [&](EGLint key, EGLint value) { a[n++] = key; a[n++] = value; };
        add(EGL_WIDTH, fr.width);
        add(EGL_HEIGHT, fr.height);
        add(EGL_LINUX_DRM_FOURCC_EXT, EGLint(fr.fourcc));
        const EGLint fd[] = {EGL_DMA_BUF_PLANE0_FD_EXT, EGL_DMA_BUF_PLANE1_FD_EXT};
        const EGLint offset[] = {EGL_DMA_BUF_PLANE0_OFFSET_EXT, EGL_DMA_BUF_PLANE1_OFFSET_EXT};
        const EGLint pitch[] = {EGL_DMA_BUF_PLANE0_PITCH_EXT, EGL_DMA_BUF_PLANE1_PITCH_EXT};
        const EGLint lo[] = {EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT, EGL_DMA_BUF_PLANE1_MODIFIER_LO_EXT};
        const EGLint hi[] = {EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT, EGL_DMA_BUF_PLANE1_MODIFIER_HI_EXT};
        for (int i = 0; i < fr.n_planes && i < 2; i++) {
            add(fd[i], fr.fds[i]);
            add(offset[i], EGLint(fr.offsets[i]));
            add(pitch[i], EGLint(fr.strides[i]));
            add(lo[i], EGLint(fr.modifier & 0xffffffff));
            add(hi[i], EGLint(fr.modifier >> 32));
        }
        a[n] = EGL_NONE;
        EGLImageKHR image = createImage(eglGetCurrentDisplay(), EGL_NO_CONTEXT, EGL_LINUX_DMA_BUF_EXT, nullptr, a);
        if (image == EGL_NO_IMAGE_KHR) {
            fprintf(stderr, "eglCreateImage failed with 0x%x for fourcc 0x%x and modifier 0x%llx\n", eglGetError(), fr.fourcc,
                    static_cast<unsigned long long>(fr.modifier));
            return 0;
        }
        GLuint texture = 0;
        f->glGenTextures(1, &texture);
        f->glBindTexture(GL_TEXTURE_EXTERNAL_OES, texture);
        f->glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        f->glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        targetTexture(GL_TEXTURE_EXTERNAL_OES, image);
        m_cache[st.st_ino] = Imported{image, texture};
        return texture;
    }

    RigelFrame m_next{}, m_current{}, m_previous{};
    QRectF m_rect;
    uint64_t m_generation = 0;
    std::map<ino_t, Imported> m_cache;
};

} // namespace

VideoItem::VideoItem(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
}

VideoItem::~VideoItem() {
    // Stops the pipeline first, so that no streaming thread can still call setFrame on this item.
    if (m_destroyStream) m_destroyStream(m_stream);
    release(m_pending);
}

void VideoItem::setStream(void *stream, void (*destroy)(void *)) {
    m_stream = stream;
    m_destroyStream = destroy;
}

void VideoItem::setStatus(bool hasVideo, bool signalLost) {
    if (m_hasVideo == hasVideo && m_signalLost == signalLost) return;
    m_hasVideo = hasVideo;
    m_signalLost = signalLost;
    emit statusChanged();
}

void VideoItem::setFrame(const RigelFrame &frame) {
    {
        QMutexLocker lock(&m_mutex);
        // A frame the scene graph didn't get to is dropped, so nothing queues up here.
        release(m_pending);
        m_pending = frame;
        m_hasPending = true;
    }
    QMetaObject::invokeMethod(this, "update", Qt::QueuedConnection);
}

QSGNode *VideoItem::updatePaintNode(QSGNode *old, UpdatePaintNodeData *) {
    auto *node = static_cast<VideoNode *>(old);
    if (!node) node = new VideoNode();
    RigelFrame frame{};
    {
        QMutexLocker lock(&m_mutex);
        if (m_hasPending) {
            frame = m_pending;
            m_pending = RigelFrame{};
            m_hasPending = false;
        }
    }
    node->setFrame(frame, QRectF(0, 0, width(), height()));
    node->markDirty(QSGNode::DirtyMaterial);
    return node;
}

extern "C" void rigel_use_gles() {
    QSurfaceFormat format = QSurfaceFormat::defaultFormat();
    format.setRenderableType(QSurfaceFormat::OpenGLES);
    QSurfaceFormat::setDefaultFormat(format);
}

extern "C" void rigel_register_types() {
    qmlRegisterType<VideoItem>("rigelcam.video", 1, 0, "VideoItem");
}

extern "C" void rigel_set_app_identity() {
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Rigelcam"));
    // Wayland compositors look up the icon through the installed .desktop file with this name.
    QGuiApplication::setDesktopFileName(QStringLiteral("rigelcam"));
    QIcon icon;
    for (int size : {16, 24, 32, 48, 64, 128, 256})
        icon.addFile(QStringLiteral(":/rigelcam/icon-%1.png").arg(size));
    QGuiApplication::setWindowIcon(icon);
}

extern "C" void rigel_item_set_frame(void *item, const RigelFrame *frame) {
    static_cast<VideoItem *>(item)->setFrame(*frame);
}

extern "C" void rigel_item_set_status(void *item, bool has_video, bool signal_lost) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "setStatus", Qt::QueuedConnection, Q_ARG(bool, has_video), Q_ARG(bool, signal_lost));
}

extern "C" void rigel_item_set_stream(void *item, void *stream, void (*destroy)(void *)) {
    static_cast<VideoItem *>(item)->setStream(stream, destroy);
}

extern "C" void *rigel_item_stream(void *item) {
    return static_cast<VideoItem *>(item)->stream();
}

#include <QGuiApplication>
#include <QIcon>
#include <QLocale>
#include <QTranslator>
#include <QtQml/QQmlEngine>
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
#include <QtCore/QRunnable>
#include <QtQuick/QQuickWindow>
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
    "precision mediump float; varying vec2 v; uniform samplerExternalOES tex; uniform float gamma;\n"
    "void main() {\n"
    "    vec4 color = texture2D(tex, v);\n"
    "    gl_FragColor = gamma == 1.0 ? color : vec4(pow(color.rgb, vec3(1.0 / gamma)), color.a);\n"
    "}\n";

struct Program {
    GLuint id = 0;
    GLint mvp = -1, tex = -1, gamma = -1, pos = -1, uv = -1;
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
        p.gamma = f->glGetUniformLocation(p.id, "gamma");
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

} // namespace

// Owned by both the item and its node, so the item can free them on the render thread once its
// stream stops. Only the render thread touches this.
struct VideoResources {
    RigelFrame next{}, current{}, previous{};
    uint64_t generation = 0;
    std::map<ino_t, Imported> cache;

    void clearCache() {
        auto *context = QOpenGLContext::currentContext();
        if (!context) return;
        static auto destroyImage = reinterpret_cast<PFNEGLDESTROYIMAGEKHRPROC>(eglGetProcAddress("eglDestroyImageKHR"));
        for (auto &[inode, imported] : cache) {
            context->functions()->glDeleteTextures(1, &imported.texture);
            destroyImage(eglGetCurrentDisplay(), imported.image);
        }
        cache.clear();
    }

    void clear() {
        clearCache();
        release(next);
        release(current);
        release(previous);
    }
};

namespace {

class ClearVideoJob : public QRunnable {
public:
    explicit ClearVideoJob(std::shared_ptr<VideoResources> resources) : m_resources(std::move(resources)) {}
    void run() override { m_resources->clear(); }

private:
    std::shared_ptr<VideoResources> m_resources;
};

class VideoNode : public QSGRenderNode {
public:
    explicit VideoNode(std::shared_ptr<VideoResources> resources) : m_res(std::move(resources)) {}
    ~VideoNode() override { m_res->clear(); }

    void setFrame(const RigelFrame &frame, const QRectF &rect, const QRectF &view, float gamma) {
        m_rect = rect;
        m_view = view;
        m_gamma = gamma;
        if (frame.release) {
            release(m_res->next);
            m_res->next = frame;
        }
    }

    StateFlags changedStates() const override { return BlendState | DepthState | ScissorState; }
    RenderingFlags flags() const override { return BoundedRectRendering | OpaqueRendering; }
    QRectF rect() const override { return m_rect; }

    void releaseResources() override { m_res->clearCache(); }

    void render(const RenderState *state) override {
        auto *f = QOpenGLContext::currentContext()->functions();
        // A frame goes back to the decoder only after a newer one was drawn, since the GPU may
        // still be sampling it.
        if (m_res->next.release) {
            release(m_res->previous);
            m_res->previous = m_res->current;
            m_res->current = m_res->next;
            m_res->next = RigelFrame{};
        }
        if (!m_res->current.release) return;
        GLuint texture = textureFor(f, m_res->current);
        if (!texture) return;

        Program &p = program(f);
        const float w = float(m_rect.width()), h = float(m_rect.height());
        const float u0 = float(m_view.left()), u1 = float(m_view.right());
        const float v0 = float(m_view.top()), v1 = float(m_view.bottom());
        const float quad[] = {0, 0, u0, v0, w, 0, u1, v0, 0, h, u0, v1, w, h, u1, v1};
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
        f->glUniform1f(p.gamma, m_gamma);
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
    // The decoder recycles a fixed set of surfaces, so each one is imported only once.
    GLuint textureFor(QOpenGLFunctions *f, const RigelFrame &fr) {
        if (fr.generation != m_res->generation) {
            m_res->clearCache();
            m_res->generation = fr.generation;
        }
        struct stat st {};
        if (fstat(fr.fds[0], &st) != 0) return 0;
        auto it = m_res->cache.find(st.st_ino);
        if (it != m_res->cache.end()) return it->second.texture;

        // More surfaces than any decoder pool uses, so a bad stream can't grow this without limit.
        if (m_res->cache.size() >= kMaxImportedSurfaces) m_res->clearCache();

        static auto createImage = reinterpret_cast<PFNEGLCREATEIMAGEKHRPROC>(eglGetProcAddress("eglCreateImageKHR"));
        static auto targetTexture = reinterpret_cast<PFNGLEGLIMAGETARGETTEXTURE2DOESPROC>(eglGetProcAddress("glEGLImageTargetTexture2DOES"));
        EGLint a[64];
        int n = 0;
        auto add = [&](EGLint key, EGLint value) { a[n++] = key; a[n++] = value; };
        add(EGL_WIDTH, fr.width);
        add(EGL_HEIGHT, fr.height);
        add(EGL_LINUX_DRM_FOURCC_EXT, EGLint(fr.fourcc));
        // Without these hints the driver assumes limited range, which crushes the blacks and
        // whites of full range cameras.
        add(EGL_YUV_COLOR_SPACE_HINT_EXT, fr.matrix == 1 ? EGL_ITU_REC709_EXT : fr.matrix == 2 ? EGL_ITU_REC2020_EXT : EGL_ITU_REC601_EXT);
        add(EGL_SAMPLE_RANGE_HINT_EXT, fr.full_range ? EGL_YUV_FULL_RANGE_EXT : EGL_YUV_NARROW_RANGE_EXT);
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
            // Every frame of the stream fails the same way, so only the first one reaches the log.
            if (!m_importFailed) {
                fprintf(stderr, "eglCreateImage failed with 0x%x for fourcc 0x%x and modifier 0x%llx\n", eglGetError(), fr.fourcc,
                        static_cast<unsigned long long>(fr.modifier));
                m_importFailed = true;
            }
            return 0;
        }
        GLuint texture = 0;
        f->glGenTextures(1, &texture);
        f->glBindTexture(GL_TEXTURE_EXTERNAL_OES, texture);
        f->glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        f->glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        targetTexture(GL_TEXTURE_EXTERNAL_OES, image);
        m_res->cache[st.st_ino] = Imported{image, texture};
        return texture;
    }

    std::shared_ptr<VideoResources> m_res;
    QRectF m_rect;
    QRectF m_view{0, 0, 1, 1};
    float m_gamma = 1;
    bool m_importFailed = false;
};

} // namespace

VideoItem::VideoItem(QQuickItem *parent) : QQuickItem(parent), m_resources(std::make_shared<VideoResources>()) {
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

void VideoItem::setView(const QRectF &view) {
    if (m_view == view) return;
    m_view = view;
    emit viewChanged();
    update();
}

void VideoItem::setGamma(qreal gamma) {
    if (m_gamma == gamma) return;
    m_gamma = gamma;
    emit gammaChanged();
    update();
}

void VideoItem::setStatus(bool hasVideo, bool signalLost) {
    if (m_hasVideo == hasVideo && m_signalLost == signalLost) return;
    m_hasVideo = hasVideo;
    m_signalLost = signalLost;
    emit statusChanged();
    // Neither video nor a reconnection means the stream was stopped.
    if (!hasVideo && !signalLost) releaseVideo();
}

// The node of a hidden item never renders again by itself, so its surfaces would stay imported.
void VideoItem::releaseVideo() {
    {
        QMutexLocker lock(&m_mutex);
        release(m_pending);
        m_hasPending = false;
    }
    if (QQuickWindow *view = window()) {
        view->scheduleRenderJob(new ClearVideoJob(m_resources), QQuickWindow::BeforeRenderingStage);
        view->update();
    }
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
    if (!node) node = new VideoNode(m_resources);
    RigelFrame frame{};
    {
        QMutexLocker lock(&m_mutex);
        if (m_hasPending) {
            frame = m_pending;
            m_pending = RigelFrame{};
            m_hasPending = false;
        }
    }
    node->setFrame(frame, QRectF(0, 0, width(), height()), m_view, float(m_gamma));
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
    qmlRegisterType<Retranslator>("rigelcam.video", 1, 0, "Retranslator");
}

void Retranslator::run() {
    if (QQmlEngine *engine = qmlEngine(this)) engine->retranslate();
}

// 1 for Portuguese, 2 for Spanish and 0 for anything else, which gets English.
extern "C" int rigel_system_language() {
    switch (QLocale::system().language()) {
    case QLocale::Portuguese:
        return 1;
    case QLocale::Spanish:
        return 2;
    default:
        return 0;
    }
}

// The source texts are English, so English needs no translator and gets an empty one here.
extern "C" void rigel_install_translation(const unsigned char *data, int size) {
    static QTranslator *current = nullptr;
    if (current) {
        QCoreApplication::removeTranslator(current);
        delete current;
        current = nullptr;
    }
    if (size == 0) return;
    auto *translator = new QTranslator(QCoreApplication::instance());
    if (translator->load(data, size)) {
        QCoreApplication::installTranslator(translator);
        current = translator;
    } else {
        delete translator;
    }
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

extern "C" void rigel_item_position(void *item, double seconds) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "playbackPosition", Qt::QueuedConnection, Q_ARG(double, seconds));
}

extern "C" void rigel_item_ended(void *item) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "playbackEnded", Qt::QueuedConnection);
}

extern "C" void rigel_item_auth_refused(void *item) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "authRefused", Qt::QueuedConnection);
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

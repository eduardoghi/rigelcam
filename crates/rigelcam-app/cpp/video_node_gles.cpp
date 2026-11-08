// Draws frames on OpenGL ES, imported from DMABuf as external textures.
#include "video_node.h"

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <GLES2/gl2ext.h>
#include <QtCore/QRunnable>
#include <QtCore/QThreadPool>
#include <QtGui/QImage>
#include <QtGui/QMatrix4x4>
#include <QtGui/QOpenGLContext>
#include <QtGui/QOpenGLFunctions>
#include <QtGui/QSurfaceFormat>
#include <QtQuick/QQuickWindow>
#include <sys/stat.h>

#include <cstdio>
#include <map>
#include <mutex>
#include <utility>

namespace {

// A square from 0 to 1 that never changes, so no draw uploads vertices. The matrix places it, and
// `view` picks the part of the frame shown, as left, top, width and height.
const char *kVertex =
    "#version 100\n"
    "attribute vec2 corner; uniform mat4 mvp; uniform vec4 view; varying vec2 v;\n"
    "void main() { v = view.xy + corner * view.zw; gl_Position = mvp * vec4(corner, 0.0, 1.0); }\n";
const float kSquare[] = {0, 0, 1, 0, 0, 1, 1, 1};
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
    GLint mvp = -1, view = -1, tex = -1, gamma = -1, corner = -1;
    GLuint vbo = 0;
    QOpenGLContext *context = nullptr;
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
Program g_program;

Program &program(QOpenGLFunctions *f) {
    Program &p = g_program;
    // A context that is gone takes the program with it. Its name alone can't tell, since a new
    // context hands out names from the start again and Qt may already hold the same one.
    QOpenGLContext *context = QOpenGLContext::currentContext();
    if (!p.id || p.context != context) {
        p = Program{};
        p.context = context;
        // Also forgotten when the context goes, in case the next one gets the same address.
        QObject::connect(context, &QOpenGLContext::aboutToBeDestroyed, [] { g_program = Program{}; });
        p.id = f->glCreateProgram();
        f->glAttachShader(p.id, compile(f, GL_VERTEX_SHADER, kVertex));
        f->glAttachShader(p.id, compile(f, GL_FRAGMENT_SHADER, kFragment));
        f->glLinkProgram(p.id);
        p.mvp = f->glGetUniformLocation(p.id, "mvp");
        p.tex = f->glGetUniformLocation(p.id, "tex");
        p.gamma = f->glGetUniformLocation(p.id, "gamma");
        p.view = f->glGetUniformLocation(p.id, "view");
        p.corner = f->glGetAttribLocation(p.id, "corner");
        // The texture always goes in unit 0, so the sampler is set once with the program.
        f->glUseProgram(p.id);
        f->glUniform1i(p.tex, 0);
        f->glGenBuffers(1, &p.vbo);
        f->glBindBuffer(GL_ARRAY_BUFFER, p.vbo);
        f->glBufferData(GL_ARRAY_BUFFER, sizeof kSquare, kSquare, GL_STATIC_DRAW);
    }
    return p;
}

constexpr size_t kMaxImportedSurfaces = 64;

struct Imported {
    EGLImageKHR image = EGL_NO_IMAGE_KHR;
    GLuint texture = 0;
};

} // namespace

// Owned by both the item and its node, so the item can free them on the render thread once its
// stream stops. Only the render thread touches the cache, and the frames are also let go from the
// interface thread while the window isn't rendered, under `frames`.
struct GlesResources : VideoResources {
    std::mutex frames;
    RigelFrame next{}, current{}, previous{};
    // The texture of `current`, found once per frame instead of on every draw, also when the
    // import failed, which would fail again on every draw.
    GLuint currentTexture = 0;
    bool currentLooked = false;
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
        currentTexture = 0;
        currentLooked = false;
    }

    void releaseFrames() override {
        std::lock_guard lock(frames);
        release(next);
        release(current);
        release(previous);
    }

    void clear() override {
        clearCache();
        releaseFrames();
    }
};

namespace {

class VideoNode : public VideoNodeBase {
public:
    VideoNode(std::shared_ptr<GlesResources> resources, std::shared_ptr<ItemLink> link)
        : m_res(std::move(resources)), m_link(std::move(link)) {}
    ~VideoNode() override { m_res->clear(); }

    void setFrame(const RigelFrame &frame, const QRectF &rect, const QRectF &view, float gamma) override {
        m_rect = rect;
        m_view = view;
        m_gamma = gamma;
        if (frame.release) {
            std::lock_guard lock(m_res->frames);
            release(m_res->next);
            m_res->next = frame;
        }
    }

    void requestPhoto(const QString &name) override { m_photoName = name; }

    StateFlags changedStates() const override { return BlendState | DepthState | ScissorState; }
    RenderingFlags flags() const override { return BoundedRectRendering | OpaqueRendering; }
    QRectF rect() const override { return m_rect; }

    void releaseResources() override { m_res->clearCache(); }

    void render(const RenderState *state) override {
        std::lock_guard lock(m_res->frames);
        auto *context = QOpenGLContext::currentContext();
        if (!context) return;
        auto *f = context->functions();
        // A frame goes back to the decoder only after a newer one was drawn, since the GPU may
        // still be sampling it.
        if (m_res->next.release) {
            release(m_res->previous);
            m_res->previous = m_res->current;
            m_res->current = m_res->next;
            m_res->next = RigelFrame{};
            m_res->currentTexture = 0;
            m_res->currentLooked = false;
        }
        if (m_res->current.release && !m_res->currentLooked) {
            m_res->currentTexture = textureFor(f, m_res->current);
            m_res->currentLooked = true;
        }
        GLuint texture = m_res->current.release ? m_res->currentTexture : 0;
        if (!texture) {
            if (!m_photoName.isEmpty()) {
                m_photoName.clear();
                m_link->report({});
            }
            return;
        }
        m_picture.shown(*m_link);

        QMatrix4x4 mvp = *state->projectionMatrix() * *matrix();
        mvp.translate(float(m_rect.x()), float(m_rect.y()));
        mvp.scale(float(m_rect.width()), float(m_rect.height()));

        f->glDisable(GL_BLEND);
        f->glDisable(GL_DEPTH_TEST);
        if (!m_photoName.isEmpty()) takePhoto(f, texture);
        // The scene graph leaves the scissor of the last clipped item on, so the first video after
        // a clipped sidebar would be cut out entirely.
        if (state->scissorEnabled()) {
            const QRect scissor = state->scissorRect();
            f->glEnable(GL_SCISSOR_TEST);
            f->glScissor(scissor.x(), scissor.y(), scissor.width(), scissor.height());
        } else {
            f->glDisable(GL_SCISSOR_TEST);
        }
        draw(f, texture, mvp);
    }

private:
    void draw(QOpenGLFunctions *f, GLuint texture, const QMatrix4x4 &mvp) {
        Program &p = program(f);
        f->glUseProgram(p.id);
        f->glUniformMatrix4fv(p.mvp, 1, GL_FALSE, mvp.constData());
        f->glUniform4f(p.view, float(m_view.x()), float(m_view.y()), float(m_view.width()), float(m_view.height()));
        f->glUniform1f(p.gamma, m_gamma);
        f->glActiveTexture(GL_TEXTURE0);
        f->glBindTexture(GL_TEXTURE_EXTERNAL_OES, texture);
        f->glBindBuffer(GL_ARRAY_BUFFER, p.vbo);
        f->glEnableVertexAttribArray(p.corner);
        f->glVertexAttribPointer(p.corner, 2, GL_FLOAT, GL_FALSE, 0, nullptr);
        f->glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        f->glDisableVertexAttribArray(p.corner);
        f->glBindTexture(GL_TEXTURE_EXTERNAL_OES, 0);
    }

    // Draws the frame again at the size of the video, since the item is usually smaller, with the
    // same shader as the screen so the photo gets the same colors.
    void takePhoto(QOpenGLFunctions *f, GLuint texture) {
        const QString name = std::exchange(m_photoName, QString());
        const int width = qMax(1, qRound(m_res->current.width * m_view.width()));
        const int height = qMax(1, qRound(m_res->current.height * m_view.height()));
        QImage image(width, height, QImage::Format_RGBX8888);

        GLint target = 0, viewport[4] = {};
        f->glGetIntegerv(GL_FRAMEBUFFER_BINDING, &target);
        f->glGetIntegerv(GL_VIEWPORT, viewport);
        GLuint color = 0, framebuffer = 0;
        f->glGenTextures(1, &color);
        f->glBindTexture(GL_TEXTURE_2D, color);
        f->glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, width, height, 0, GL_RGBA, GL_UNSIGNED_BYTE, nullptr);
        f->glBindTexture(GL_TEXTURE_2D, 0);
        f->glGenFramebuffers(1, &framebuffer);
        f->glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);
        f->glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, color, 0);
        const bool ready = !image.isNull() && f->glCheckFramebufferStatus(GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE;
        if (ready) {
            // The square fills the target. The first row glReadPixels returns is the bottom of the
            // target, so the top of the video goes there and the image comes out upright.
            QMatrix4x4 fill;
            fill.translate(-1, -1);
            fill.scale(2, 2);
            f->glViewport(0, 0, width, height);
            f->glDisable(GL_SCISSOR_TEST);
            draw(f, texture, fill);
            f->glReadPixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, image.bits());
        }
        f->glBindFramebuffer(GL_FRAMEBUFFER, GLuint(target));
        f->glViewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        f->glDeleteFramebuffers(1, &framebuffer);
        f->glDeleteTextures(1, &color);

        if (!ready) {
            m_link->report({});
            return;
        }
        savePhotoImage(image, name, m_link);
    }

    // The decoder recycles a fixed set of surfaces, so each one is imported only once.
    GLuint textureFor(QOpenGLFunctions *f, const RigelFrame &fr) {
        if (fr.generation != m_res->generation) {
            m_res->clearCache();
            m_res->generation = fr.generation;
        }
        struct stat st {};
        if (fstat(fr.fds[0], &st) != 0) {
            // Without this the tile would stay black while the stream counts as playing.
            if (m_picture.failed(*m_link)) fprintf(stderr, "a frame of the decoder couldn't be read\n");
            return 0;
        }
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
            if (m_picture.failed(*m_link)) {
                fprintf(stderr, "eglCreateImage failed with 0x%x for fourcc 0x%x and modifier 0x%llx\n", eglGetError(), fr.fourcc,
                        static_cast<unsigned long long>(fr.modifier));
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

    std::shared_ptr<GlesResources> m_res;
    std::shared_ptr<ItemLink> m_link;
    QString m_photoName;
    QRectF m_rect;
    QRectF m_view{0, 0, 1, 1};
    float m_gamma = 1;
    PictureReport m_picture;
};

} // namespace

std::shared_ptr<VideoResources> createVideoResources() { return std::make_shared<GlesResources>(); }

VideoNodeBase *createVideoNode(std::shared_ptr<VideoResources> resources, std::shared_ptr<ItemLink> link) {
    return new VideoNode(std::static_pointer_cast<GlesResources>(std::move(resources)), std::move(link));
}

void prepareGraphics() {
    QSurfaceFormat format = QSurfaceFormat::defaultFormat();
    format.setRenderableType(QSurfaceFormat::OpenGLES);
    QSurfaceFormat::setDefaultFormat(format);
}

bool hasSuperResolution() { return false; }
bool hasRtxHdr() { return false; }
QSGNode *createHdrPassNode() { return nullptr; }

void prepareWindow(QQuickWindow *) {}

extern "C" bool rigel_graphics_ready() { return true; }

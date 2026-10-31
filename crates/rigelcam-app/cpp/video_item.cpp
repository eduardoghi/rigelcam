#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QIcon>
#include <QImage>
#include <QImageWriter>
#include <QLocale>
#include <QRegularExpression>
#include <QStandardPaths>
#include <QThreadPool>
#include <QTranslator>
#include <QtQml/QQmlEngine>
#include "video_item.h"
#include "video_node.h"

#include <QtQml/qqml.h>
#include <QtCore/QRunnable>
#include <QtQuick/QQuickWindow>

#include <cstdio>
#include <utility>

namespace {

// Returns the path, or an empty string on failure. Runs off the render thread, since encoding takes
// longer than a frame. Two photos in the same second get a number instead of overwriting each other.
QString writePhoto(const QImage &image, QString name) {
    const QString folder = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation) + QStringLiteral("/Rigelcam");
    if (!QDir().mkpath(folder)) return {};
    // Camera names come from the recorder, so they can't choose the folder or hide the file.
    static const QRegularExpression notAllowed(QStringLiteral("[<>:\"/\\\\|?*\\x00-\\x1f\\x7f]"));
    name.replace(notAllowed, QStringLiteral("_"));
    while (name.startsWith(u'.')) name.remove(0, 1);
    name.truncate(150);
    for (int copy = 1; copy < 100; copy++) {
        const QString suffix = copy == 1 ? QString() : QStringLiteral(" (%1)").arg(copy);
        QFile file(folder + u'/' + name + suffix + QStringLiteral(".jpg"));
        if (!file.open(QIODevice::WriteOnly | QIODevice::NewOnly)) {
            if (file.exists()) continue;
            return {};
        }
        QImageWriter writer(&file, "jpeg");
        writer.setQuality(95);
        if (writer.write(image) && file.flush()) return file.fileName();
        file.remove();
        return {};
    }
    return {};
}

} // namespace

void ItemLink::report(const QUrl &file) {
    QMutexLocker lock(&mutex);
    if (item) QMetaObject::invokeMethod(item, "photoFinished", Qt::QueuedConnection, Q_ARG(QUrl, file));
}

void ItemLink::reportNoPicture() {
    QMutexLocker lock(&mutex);
    if (item) QMetaObject::invokeMethod(item, "setStatus", Qt::QueuedConnection, Q_ARG(bool, false), Q_ARG(bool, true));
}

void savePhotoImage(const QImage &image, const QString &name, const std::shared_ptr<ItemLink> &link) {
    QThreadPool::globalInstance()->start([image, name, link] {
        const QString path = writePhoto(image, name);
        link->report(path.isEmpty() ? QUrl() : QUrl::fromLocalFile(path));
    });
}

namespace {

class ClearVideoJob : public QRunnable {
public:
    explicit ClearVideoJob(std::shared_ptr<VideoResources> resources) : m_resources(std::move(resources)) {}
    void run() override { m_resources->clear(); }

private:
    std::shared_ptr<VideoResources> m_resources;
};

} // namespace

VideoItem::VideoItem(QQuickItem *parent)
    : QQuickItem(parent), m_resources(createVideoResources()), m_link(std::make_shared<ItemLink>()) {
    m_link->item = this;
    setFlag(ItemHasContents, true);
}

VideoItem::~VideoItem() {
    // Stops the pipeline first, so that no streaming thread can still call setFrame on this item.
    if (m_destroyStream) m_destroyStream(m_stream);
    release(m_pending);
    QMutexLocker lock(&m_link->mutex);
    m_link->item = nullptr;
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

void VideoItem::savePhoto(const QString &name) {
    if (!m_hasVideo) {
        emit photoFinished({});
        return;
    }
    m_photoName = name;
    // A paused recording sends no frames, so nothing else would render this item.
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
    auto *node = static_cast<VideoNodeBase *>(old);
    if (!node) node = createVideoNode(m_resources, m_link);
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
    if (!m_photoName.isEmpty()) node->requestPhoto(std::exchange(m_photoName, QString()));
    node->markDirty(QSGNode::DirtyMaterial);
    return node;
}

extern "C" void rigel_prepare_graphics() { prepareGraphics(); }

extern "C" bool rigel_has_window() { return !QGuiApplication::topLevelWindows().isEmpty(); }

void GraphicsSetup::prepare(QQuickWindow *window) { prepareWindow(window); }

extern "C" void rigel_register_types() {
    qmlRegisterType<VideoItem>("rigelcam.video", 1, 0, "VideoItem");
    qmlRegisterType<Retranslator>("rigelcam.video", 1, 0, "Retranslator");
    qmlRegisterType<GraphicsSetup>("rigelcam.video", 1, 0, "GraphicsSetup");
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

extern "C" void rigel_item_position(void *item, double seconds, int attempt) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "playbackPosition", Qt::QueuedConnection, Q_ARG(double, seconds), Q_ARG(int, attempt));
}

extern "C" void rigel_item_ended(void *item, int attempt) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "playbackEnded", Qt::QueuedConnection, Q_ARG(int, attempt));
}

extern "C" void rigel_item_failed(void *item, int attempt) {
    QMetaObject::invokeMethod(static_cast<VideoItem *>(item), "playbackFailed", Qt::QueuedConnection, Q_ARG(int, attempt));
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

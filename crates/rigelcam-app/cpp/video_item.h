#pragma once

#include <QtCore/QMutex>
#include <QtCore/QUrl>
#include <QtQuick/QQuickItem>
#include <cstdint>
#include <memory>

extern "C" {
struct RigelFrame {
    int width;
    int height;
    uint32_t fourcc;
    uint64_t modifier;
    // Changes when the pipeline is rebuilt, since its surfaces may reuse old inode numbers.
    uint64_t generation;
    int n_planes;
    int full_range;
    // 0 is BT.601, 1 is BT.709 and 2 is BT.2020.
    int matrix;
    int fds[4];
    uint32_t offsets[4];
    uint32_t strides[4];
    void *owner;
    void (*release)(void *);
};

void rigel_use_gles();
void rigel_register_types();
void rigel_set_app_identity();
int rigel_system_language();
void rigel_install_translation(const unsigned char *data, int size);
void rigel_item_set_frame(void *item, const RigelFrame *frame);
void rigel_item_set_status(void *item, bool has_video, bool signal_lost);
void rigel_item_auth_refused(void *item);
void rigel_item_position(void *item, double seconds, int attempt);
void rigel_item_ended(void *item, int attempt);
void rigel_item_failed(void *item, int attempt);
void rigel_item_set_stream(void *item, void *stream, void (*destroy)(void *));
void *rigel_item_stream(void *item);
}

/// Lets QML ask for its texts again after the language changes.
class Retranslator : public QObject {
    Q_OBJECT
public:
    using QObject::QObject;
    Q_INVOKABLE void run();
};

struct VideoResources;
struct PhotoLink;

/// QML item that draws the latest DMABuf frame it received, imported as an external texture.
class VideoItem : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(bool hasVideo READ hasVideo NOTIFY statusChanged)
    Q_PROPERTY(bool signalLost READ signalLost NOTIFY statusChanged)
    /// Part of the picture to show, in fractions of its size, which is how digital zoom works.
    Q_PROPERTY(QRectF view READ view WRITE setView NOTIFY viewChanged)
    /// Lifts the midtones above 1 and lowers them below it, keeping black and white in place.
    Q_PROPERTY(qreal gamma READ gamma WRITE setGamma NOTIFY gammaChanged)
public:
    explicit VideoItem(QQuickItem *parent = nullptr);
    ~VideoItem() override;
    void setFrame(const RigelFrame &frame);
    bool hasVideo() const { return m_hasVideo; }
    bool signalLost() const { return m_signalLost; }
    QRectF view() const { return m_view; }
    void setView(const QRectF &view);
    qreal gamma() const { return m_gamma; }
    void setGamma(qreal gamma);
    Q_INVOKABLE void setStatus(bool hasVideo, bool signalLost);
    void setStream(void *stream, void (*destroy)(void *));
    void *stream() const { return m_stream; }
    /// Saves what the item shows in the pictures folder, at the resolution of the video and with
    /// the view and gamma applied. Reports through photoFinished. The name becomes the file name.
    Q_INVOKABLE void savePhoto(const QString &name);

signals:
    void statusChanged();
    void viewChanged();
    void gammaChanged();
    /// The recorder refused the user or password, and this stream stopped trying.
    void authRefused();
    /// Seconds of a recording played since the point it was opened at. Each signal carries the
    /// attempt that `play` returned, because a replaced stream still reports for a moment.
    void playbackPosition(double seconds, int attempt);
    /// The recording reached the end of what the recorder has.
    void playbackEnded(int attempt);
    /// The recorder couldn't open a recording or stopped sending it.
    void playbackFailed(int attempt);
    /// The file of the photo, or an empty URL if it couldn't be saved.
    void photoFinished(const QUrl &file);

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;

private:
    void releaseVideo();

    std::shared_ptr<VideoResources> m_resources;
    std::shared_ptr<PhotoLink> m_photoLink;
    QString m_photoName;
    QMutex m_mutex;
    RigelFrame m_pending{};
    bool m_hasPending = false;
    bool m_hasVideo = false;
    bool m_signalLost = false;
    QRectF m_view{0, 0, 1, 1};
    qreal m_gamma = 1;
    void *m_stream = nullptr;
    void (*m_destroyStream)(void *) = nullptr;
};

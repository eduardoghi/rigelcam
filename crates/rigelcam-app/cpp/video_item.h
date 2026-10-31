#pragma once

#include <QtCore/QMutex>
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
bool rigel_system_is_portuguese();
void rigel_install_translation(const unsigned char *data, int size);
void rigel_item_set_frame(void *item, const RigelFrame *frame);
void rigel_item_set_status(void *item, bool has_video, bool signal_lost);
void rigel_item_auth_refused(void *item);
void rigel_item_position(void *item, double seconds);
void rigel_item_ended(void *item);
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

/// QML item that draws the latest DMABuf frame it received, imported as an external texture.
class VideoItem : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(bool hasVideo READ hasVideo NOTIFY statusChanged)
    Q_PROPERTY(bool signalLost READ signalLost NOTIFY statusChanged)
    /// Part of the picture to show, in fractions of its size, which is how digital zoom works.
    Q_PROPERTY(QRectF view READ view WRITE setView NOTIFY viewChanged)
public:
    explicit VideoItem(QQuickItem *parent = nullptr);
    ~VideoItem() override;
    void setFrame(const RigelFrame &frame);
    bool hasVideo() const { return m_hasVideo; }
    bool signalLost() const { return m_signalLost; }
    QRectF view() const { return m_view; }
    void setView(const QRectF &view);
    Q_INVOKABLE void setStatus(bool hasVideo, bool signalLost);
    void setStream(void *stream, void (*destroy)(void *));
    void *stream() const { return m_stream; }

signals:
    void statusChanged();
    void viewChanged();
    /// The recorder refused the user or password, and this stream stopped trying.
    void authRefused();
    /// Seconds of a recording played since the point it was opened at.
    void playbackPosition(double seconds);
    /// The recording reached the end of what the recorder has.
    void playbackEnded();

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;

private:
    void releaseVideo();

    std::shared_ptr<VideoResources> m_resources;
    QMutex m_mutex;
    RigelFrame m_pending{};
    bool m_hasPending = false;
    bool m_hasVideo = false;
    bool m_signalLost = false;
    QRectF m_view{0, 0, 1, 1};
    void *m_stream = nullptr;
    void (*m_destroyStream)(void *) = nullptr;
};

#pragma once

#include <QtCore/QMutex>
#include <QtQuick/QQuickItem>
#include <cstdint>

extern "C" {
struct RigelFrame {
    int width;
    int height;
    uint32_t fourcc;
    uint64_t modifier;
    // Changes when the pipeline is rebuilt, since its surfaces may reuse old inode numbers.
    uint64_t generation;
    int n_planes;
    int fds[4];
    uint32_t offsets[4];
    uint32_t strides[4];
    void *owner;
    void (*release)(void *);
};

void rigel_use_gles();
void rigel_register_types();
void rigel_set_app_identity();
void rigel_item_set_frame(void *item, const RigelFrame *frame);
void rigel_item_set_status(void *item, bool has_video, bool signal_lost);
void rigel_item_set_stream(void *item, void *stream, void (*destroy)(void *));
void *rigel_item_stream(void *item);
}

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

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;

private:
    QMutex m_mutex;
    RigelFrame m_pending{};
    bool m_hasPending = false;
    bool m_hasVideo = false;
    bool m_signalLost = false;
    QRectF m_view{0, 0, 1, 1};
    void *m_stream = nullptr;
    void (*m_destroyStream)(void *) = nullptr;
};

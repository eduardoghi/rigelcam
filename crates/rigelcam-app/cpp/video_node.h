#pragma once

#include "video_item.h"

#include <QtCore/QMutex>
#include <QtCore/QRectF>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtQuick/QSGRenderNode>
#include <memory>

/// What a node keeps on the GPU, owned with the item so it can be freed once the stream stops.
struct VideoResources {
    virtual ~VideoResources() = default;
    /// Runs on the render thread.
    virtual void clear() = 0;
};

/// Reaches the item from the render thread and the writer, which can outlive it.
struct ItemLink {
    QMutex mutex;
    VideoItem *item = nullptr;

    void report(const QUrl &file);
    /// Frames arrive but can't be shown, which the item has to stop presenting as live.
    void reportNoPicture();
};

/// Encodes a photo off the render thread and reports the file.
void savePhotoImage(const QImage &image, const QString &name, const std::shared_ptr<ItemLink> &link);

inline void release(RigelFrame &frame) {
    if (frame.release) frame.release(frame.owner);
    frame = RigelFrame{};
}

class VideoNodeBase : public QSGRenderNode {
public:
    /// Takes `frame` when it has a release function, which an empty update doesn't.
    virtual void setFrame(const RigelFrame &frame, const QRectF &rect, const QRectF &view, float gamma) = 0;
    virtual void requestPhoto(const QString &name) = 0;
};

// One file of the build implements these for each system.
std::shared_ptr<VideoResources> createVideoResources();
VideoNodeBase *createVideoNode(std::shared_ptr<VideoResources> resources, std::shared_ptr<ItemLink> link);
void prepareGraphics();
void prepareWindow(QQuickWindow *window);

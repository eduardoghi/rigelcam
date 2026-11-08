import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import rigelcam.video 1.0
import rigelcam
import "playback.js" as Playback

ApplicationWindow {
    id: win
    width: 1280
    height: 800
    // Below this the top bar is cut in the languages with the longest texts.
    minimumWidth: 900
    minimumHeight: 600
    color: colors.page
    title: "Rigelcam"

    QtObject {
        id: colors
        readonly property bool dark: grid.theme ? grid.theme === "dark" : Qt.styleHints.colorScheme !== Qt.Light
        readonly property url logo: dark ? "qrc:/rigelcam/logo.svg" : "qrc:/rigelcam/logo-light.svg"
        readonly property color page: dark ? "#111214" : "#F2F2F3"
        readonly property color bar: dark ? "#17181B" : "#FFFFFF"
        readonly property color surface: dark ? "#1C1D21" : "#F7F7F8"
        readonly property color raised: dark ? "#26282D" : "#E8E9EB"
        readonly property color line: dark ? "#2E3036" : "#D9DADD"
        readonly property color text: dark ? "#ECEDEE" : "#17181B"
        readonly property color muted: dark ? "#9A9DA3" : "#5E6168"
        readonly property color faint: dark ? "#6B6E75" : "#8A8D93"
        readonly property color accent: dark ? "#2F7BEF" : "#1F66D1"
        readonly property color accentHover: dark ? "#4A8EF2" : "#2F7BEF"
        readonly property color danger: dark ? "#E5484D" : "#CE2C31"
        readonly property color live: dark ? "#3DD68C" : "#1F9D63"
        readonly property color waiting: dark ? "#E5B94D" : "#A86B00"
        readonly property color panel: dark ? "#D917181B" : "#E6FFFFFF"
        // Over the video, which looks the same in both themes.
        readonly property color overVideo: "#B3111214"
        readonly property color overVideoText: "#ECEDEE"
    }

    component AppButton: Button {
        id: button
        property color fill: "transparent"
        property color hoverFill: colors.raised
        property color edge: "transparent"
        property color contentColor: colors.text
        implicitHeight: 34
        leftPadding: 12
        rightPadding: 12
        topPadding: 0
        bottomPadding: 0
        spacing: 8
        font.pixelSize: 13
        font.weight: Font.Medium
        icon.width: 16
        icon.height: 16
        icon.color: contentColor
        palette.buttonText: contentColor
        opacity: enabled ? 1 : 0.45
        background: Rectangle {
            radius: 4
            color: button.hovered || button.visualFocus ? button.hoverFill : button.fill
            border.color: button.edge
        }
    }

    component AppToolTip: ToolTip {
        id: tip
        delay: 400
        padding: 7
        contentItem: Text {
            text: tip.text
            wrapMode: Text.WordWrap
            font.pixelSize: 12
            color: colors.text
        }
        background: Rectangle {
            radius: 4
            color: colors.raised
            border.color: colors.line
        }
    }

    // A button with only an icon in the toolbars, its text shown as a tip under the mouse.
    component IconButton: AppButton {
        id: iconButton
        property string tip: text
        property bool tipVisible: hovered
        display: AbstractButton.IconOnly
        implicitWidth: 34
        leftPadding: 9
        rightPadding: 9
        AppToolTip {
            visible: iconButton.tipVisible
            text: iconButton.tip
        }
    }

    component PrimaryButton: AppButton {
        contentColor: "#FFFFFF"
        fill: colors.accent
        hoverFill: colors.accentHover
    }

    component AppComboBox: ComboBox {
        id: box
        Connections {
            target: box.popup
            function onAboutToShow() {
                win.comboOpen = true
            }
            function onAboutToHide() {
                win.comboOpen = false
            }
        }
        // The text of a row, as a function so a list of labels doesn't have to be rebuilt.
        property var labelOf: item => item[textRole]
        displayText: currentIndex >= 0 ? labelOf(model[currentIndex]) : ""
        implicitHeight: 34
        font.pixelSize: 13
        font.weight: Font.Medium
        leftPadding: 0
        palette.text: colors.text
        palette.buttonText: colors.text
        background: Rectangle {
            radius: 4
            // A flat one, as in the toolbar, only lights up under the mouse.
            color: box.hovered ? colors.raised : box.flat ? "transparent" : colors.surface
            border.color: box.flat ? "transparent" : colors.line
        }
        contentItem: Text {
            leftPadding: 12
            rightPadding: 28
            verticalAlignment: Text.AlignVCenter
            text: box.displayText
            textFormat: Text.PlainText
            font: box.font
            color: colors.text
            elide: Text.ElideRight
        }
        indicator: Image {
            x: box.width - width - 8
            anchors.verticalCenter: parent.verticalCenter
            source: "qrc:/rigelcam/icons/chevron.svg"
            sourceSize.width: 16
            sourceSize.height: 16
        }
        delegate: ItemDelegate {
            id: boxRow
            required property var modelData
            required property int index
            width: ListView.view.width
            height: 34
            highlighted: box.highlightedIndex === index
            background: Rectangle {
                radius: 3
                color: boxRow.highlighted ? colors.raised : "transparent"
            }
            contentItem: Text {
                text: box.labelOf(boxRow.modelData)
                textFormat: Text.PlainText
                font.pixelSize: 13
                color: colors.text
                verticalAlignment: Text.AlignVCenter
            }
        }
        popup.y: box.height + 4
        popup.padding: 4
        // Keeps a long list inside the window, where it scrolls.
        popup.margins: 8
        // The default height leaves out the padding, which would make the list scroll.
        popup.height: popup.contentItem.implicitHeight + 2 * popup.padding
        popup.background: Rectangle {
            radius: 5
            color: colors.surface
            border.color: colors.line
        }
    }

    // Digital zoom of a video, with the top left of the visible part in fractions.
    component VideoZoom: QtObject {
        property real scale: 1
        property real left: 0
        property real top: 0
        readonly property rect view: Qt.rect(left, top, 1 / scale, 1 / scale)

        function reset() {
            scale = 1
            left = 0
            top = 0
        }

        function panTo(x, y) {
            const limit = 1 - 1 / scale
            left = Math.min(Math.max(x, 0), limit)
            top = Math.min(Math.max(y, 0), limit)
        }

        // Keeps the point at (px, py) of the view, in fractions, under the cursor.
        function zoomAt(newScale, px, py) {
            const x = left + px / scale
            const y = top + py / scale
            scale = Math.min(Math.max(newScale, 1), 8)
            panTo(x - px / scale, y - py / scale)
        }
    }

    component ZoomGestures: Item {
        id: gestures
        required property VideoZoom zoom
        anchors.fill: parent

        HoverHandler {
            cursorShape: gestures.zoom.scale > 1 ? Qt.OpenHandCursor : Qt.ArrowCursor
        }
        WheelHandler {
            target: null
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => gestures.zoom.zoomAt(gestures.zoom.scale * Math.pow(1.0015, event.angleDelta.y),
                                                   event.x / gestures.width, event.y / gestures.height)
        }
        DragHandler {
            property real startX
            property real startY
            enabled: gestures.zoom.scale > 1
            target: null
            onActiveChanged: {
                startX = gestures.zoom.left
                startY = gestures.zoom.top
            }
            onActiveTranslationChanged: {
                if (active)
                    gestures.zoom.panTo(startX - activeTranslation.x / gestures.width / gestures.zoom.scale,
                                        startY - activeTranslation.y / gestures.height / gestures.zoom.scale)
            }
        }
    }

    // Drags with a floating label, so the tile or row dragged stays where it is.
    component CameraDrag: DragHandler {
        property var camera
        target: null
        grabPermissions: PointerHandler.CanTakeOverFromAnything
        onActiveChanged: active ? win.beginDrag(camera, centroid.scenePosition) : win.endDrag()
        onCentroidChanged: if (active) win.moveDrag(centroid.scenePosition)
        // The row or tile may go away mid-drag, and then activeChanged never comes.
        Component.onDestruction: if (active) win.dragged = null
    }

    component GammaControl: RowLayout {
        // Without the label and with a shorter slider, for a bar that is running out of room.
        property bool compact: false
        spacing: 6
        Text {
            visible: !parent.compact
            font.pixelSize: 13
            color: colors.text
            text: qsTr("Gamma")
        }
        Slider {
            id: gammaSlider
            Layout.preferredWidth: parent.compact ? 70 : 110
            onActiveFocusChanged: win.gammaFocused = activeFocus
            from: 0.5
            to: 3
            stepSize: 0.05
            value: win.gamma
            onMoved: win.gamma = value
            background: Rectangle {
                x: gammaSlider.leftPadding
                y: (gammaSlider.height - height) / 2
                width: gammaSlider.availableWidth
                height: 4
                radius: 2
                color: colors.line
                Rectangle {
                    width: gammaSlider.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    color: colors.accent
                }
            }
            handle: Rectangle {
                x: gammaSlider.leftPadding + gammaSlider.visualPosition * (gammaSlider.availableWidth - width)
                y: (gammaSlider.height - height) / 2
                width: 14
                height: 14
                radius: 7
                color: gammaSlider.pressed ? colors.accentHover : colors.accent
            }
        }
        AppButton {
            id: gammaReset
            text: win.gamma.toFixed(2)
            implicitWidth: 44
            leftPadding: 4
            rightPadding: 4
            onClicked: win.gamma = 1
            AppToolTip {
                visible: gammaReset.hovered
                text: qsTr("Back to normal")
            }
        }
    }

    component FormField: TextField {
        id: field
        Layout.fillWidth: true
        implicitHeight: 38
        leftPadding: 12
        rightPadding: 12
        font.pixelSize: 14
        color: colors.text
        placeholderTextColor: colors.faint
        selectionColor: colors.accent
        background: Rectangle {
            radius: 4
            color: colors.page
            border.color: field.activeFocus ? colors.accent : colors.line
        }
    }

    component FormLabel: Label {
        font.pixelSize: 12
        color: colors.muted
        Layout.topMargin: 6
    }

    component RtxButton: AppButton {
        id: rtxButton
        property bool on: false
        property string tip
        icon.source: on ? "qrc:/rigelcam/icons/check.svg" : ""
        fill: on ? colors.accent : "transparent"
        hoverFill: on ? colors.accentHover : colors.raised
        edge: on ? colors.accent : colors.line
        contentColor: on ? "#FFFFFF" : colors.muted
        AppToolTip {
            visible: rtxButton.hovered
            width: 340
            text: rtxButton.tip
        }
    }

    component SuperResolutionButton: RtxButton {
        visible: graphicsSetup.superResolution
        text: "RTX Super Resolution"
        on: win.superResolution
        tip: qsTr("Uses NVIDIA RTX Video, which has to be on in the driver. Detail is rebuilt by AI, so don't rely on it to read plates or faces.")
        onClicked: win.superResolution = !win.superResolution
    }

    component RtxHdrButton: RtxButton {
        visible: graphicsSetup.rtxHdr
        text: "RTX HDR"
        on: win.rtxHdr
        tip: qsTr("Uses NVIDIA RTX Video, which has to be on in the driver. Brightness and color are estimated by AI for HDR screens.")
        onClicked: win.rtxHdr = !win.rtxHdr
    }

    component AppMenu: Menu {
        padding: 4
        // Takes the keys of the player while open, like a list.
        onAboutToShow: win.comboOpen = true
        onAboutToHide: win.comboOpen = false
        background: Rectangle {
            implicitWidth: 200
            radius: 5
            color: colors.surface
            border.color: colors.line
        }
    }

    component AppMenuItem: MenuItem {
        id: menuItem
        implicitHeight: 34
        leftPadding: 10
        spacing: 10
        font.pixelSize: 13
        icon.width: 16
        icon.height: 16
        // The style paints the text and the icon with these, highlighted or not.
        icon.color: colors.text
        palette.windowText: colors.text
        palette.highlightedText: colors.text
        background: Rectangle {
            radius: 3
            color: menuItem.highlighted ? colors.raised : "transparent"
        }
    }

    // The name of a camera over its video, with square corners like the text a camera prints.
    component OverlayLabel: Rectangle {
        property alias text: overlayText.text
        property real maxWidth: Infinity
        width: Math.min(overlayText.implicitWidth + 14, maxWidth)
        height: overlayText.implicitHeight + 6
        color: colors.overVideo
        Text {
            id: overlayText
            anchors.fill: parent
            anchors.leftMargin: 7
            anchors.rightMargin: 7
            verticalAlignment: Text.AlignVCenter
            // Names come from the recorder, so they can't be read as markup.
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pixelSize: 12
            font.weight: Font.Medium
            color: colors.overVideoText
        }
    }

    component DangerButton: AppButton {
        fill: colors.danger
        hoverFill: "#F2666A"
        contentColor: "#FFFFFF"
    }

    // Asks before something that can't be undone, with the button that does it in red.
    component ConfirmDialog: AppDialog {
        id: confirm
        property string message
        signal confirmed()
        width: Math.min(400, parent.width - 32)
        ColumnLayout {
            anchors.fill: parent
            spacing: 18
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 14
                color: colors.muted
                text: confirm.message
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                AppButton { text: qsTr("Cancel"); onClicked: confirm.close() }
                DangerButton {
                    text: qsTr("Delete")
                    onClicked: {
                        confirm.confirmed()
                        confirm.close()
                    }
                }
            }
        }
    }

    component AppDialog: Dialog {
        id: dialog
        modal: true
        anchors.centerIn: parent
        padding: 24
        topPadding: 8
        Overlay.modal: Rectangle { color: "#B30A0A0B" }
        background: Rectangle {
            radius: 8
            color: colors.surface
            border.color: colors.line
        }
        header: Label {
            text: dialog.title
            textFormat: Text.PlainText
            font.pixelSize: 18
            font.weight: Font.DemiBold
            color: colors.text
            padding: 24
            bottomPadding: 8
        }
    }

    CameraGrid {
        id: grid
        Component.onCompleted: load()
        onTreeChanged: win.syncTree()
        onLayoutChanged: win.syncTiles(JSON.parse(grid.tree || "[]"))
        onLanguageChanged: retranslator.run()
    }

    Retranslator { id: retranslator }

    readonly property var recorderList: grid.tree ? JSON.parse(grid.tree) : []
    readonly property var gridLayout: grid.layout ? JSON.parse(grid.layout) : ({ columns: 1, cameras: [] })
    readonly property var gridKeys: gridLayout.cameras.map(c => c ? cameraKey(c.recorder, c.channel) : null)
    readonly property var tileKeys: recorderList.reduce((keys, r) => keys.concat(r.cameras.map(c => cameraKey(r.id, c.number))), [])
    readonly property int cols: gridLayout.columns
    // Below this the top bar doesn't fit, so the logo becomes the icon and the count goes.
    readonly property bool narrowBar: width < 1150
    // The saved grids as { name }, the one on screen at grid.currentLayout.
    readonly property var layoutNames: grid.layouts ? JSON.parse(grid.layouts) : []
    readonly property bool layoutKeys: !inRecordings && layoutNames.length > 1 && zoomed === "" && dragged === null
        && !popupOpen
    readonly property int includedCount: gridKeys.filter(k => k !== null).length
    property int liveCount: 0
    // State of each camera in the grid, by key, as "live", "denied", "lost" or "connecting".
    property var cameraStatus: ({})
    signal photoRequested()
    // Closing waits for the cancelled export, which removes its unfinished file.
    property bool closeAfterExport: false
    // The installer closes the app, which would lose the video being exported, so it waits.
    property bool updateAfterExport: false
    onClosing: close => {
        if (!grid.exportRunning)
            return
        closeAfterExport = true
        grid.cancelExports()
        close.accepted = false
    }
    Connections {
        target: grid
        function onExportRunningChanged() {
            if (win.closeAfterExport && !grid.exportRunning)
                win.close()
            // Between two exports of the queue it turns false for a moment, with others waiting.
            if (win.updateAfterExport && !grid.exportRunning && grid.exportsWaiting === 0) {
                win.updateAfterExport = false
                grid.installUpdate()
            }
        }
        function onUpdateStarted() {
            Qt.quit()
        }
        function onExportFinished(folder, file, error) {
            photoToast.showVideo(folder, file, error)
        }
    }

    // Key of the camera shown alone after a double click, or empty for the whole grid.
    property string zoomed: ""
    property string zoomedName: ""
    // Only for a camera seen alone, where someone is looking for detail. Not saved, because the
    // right value depends on the camera and on the hour.
    property real gamma: 1
    // Not saved, so nobody ends up watching rebuilt detail without having asked for it.
    property bool superResolution: false
    property bool rtxHdr: false
    function isoDay(date) {
        return Playback.isoDay(date)
    }
    property var zoomedCamera: null
    readonly property bool fullScreen: visibility === Window.FullScreen
    // Nothing is seen, so the video stops using the decoder and the network.
    readonly property bool minimized: visibility === Window.Minimized || !graphicsSetup.exposed
    onMinimizedChanged: playback.windowMinimized(minimized)
    // Where F11 goes back to, since the window may have been maximized or not.
    property int windowedVisibility: Window.Maximized
    onFullScreenChanged: if (fullScreen) fullScreenHint.restart()
    // The button that leaves the recordings only exists with a recorder.
    onRecorderListChanged: if (recorderList.length === 0) setMode("live")
    // Of the camera shown alone. Each cell of the recordings has its own.
    readonly property VideoZoom zoom: VideoZoom {}
    onZoomedChanged: zoom.reset()
    // "live" or "recordings", which stops the live cameras to free the network and the decoder.
    property string mode: "live"
    readonly property bool inRecordings: mode === "recordings"
    // Off while a popup is open, which takes these keys for itself.
    readonly property bool playerKeys: inRecordings && playback.spans.length > 0 && !popupOpen
    // Any dialog, list or menu, which takes the keys for itself. One list, so a new popup
    // can't be left out of one shortcut and not another.
    readonly property bool popupOpen: comboOpen || aboutDialog.visible || addDialog.visible
        || exportDialog.visible || layoutDialog.visible || removeLayoutDialog.visible
        || removeDialog.visible || calendar.visible || eventsPopup.visible
    property bool comboOpen: false
    property bool gammaFocused: false
    // The player controls don't fit beside the sidebar, so it opens over the video on request.
    readonly property bool narrowPlayer: inRecordings && width < playerRow.implicitWidth + 300
    property bool sidebarFloating: false
    // The cameras in the recordings as { recorder, number, name }, in fixed slots that can be empty,
    // so taking one out doesn't touch the streams of the others.
    property var playbackCameras: [null, null, null, null]
    readonly property var playbackCamera: playbackCameras.find(c => c !== null) ?? null
    // The camera being dragged to a square.
    property var dragged: null
    // Recorders with their cameras folded in the sidebar, for this session.
    property var foldedRecorders: []

    // Messages from Rust carry their arguments after the text. Reading the language makes bindings
    // to this function follow a change of it.
    function msg(text) {
        if (!text || !grid.language)
            return text
        const parts = text.split("\u001f")
        // In one pass, unlike chained arg(), so a %2 inside an error of the recorder stays as it is.
        return qsTranslate("Main", parts[0]).replace(/%(\d+)/g,
                                                     (mark, n) => n >= 1 && n < parts.length ? parts[n] : mark)
    }

    // A grid without a name is called by its place in the list. Reading the language makes bindings
    // to this follow a change of it, as in msg().
    function layoutLabel(index, name) {
        if (name || !grid.language)
            return name
        return qsTr("Grid %1").arg(index + 1)
    }

    function stepLayout(step) {
        const count = layoutNames.length
        grid.selectLayout((grid.currentLayout + step + count) % count)
    }

    function secondsOf(text) {
        return Playback.secondsOf(text)
    }

    function clamp(value, low, high) {
        return Math.min(Math.max(value, low), high)
    }

    function toggleFullScreen() {
        if (fullScreen) {
            visibility = windowedVisibility
        } else {
            windowedVisibility = visibility
            visibility = Window.FullScreen
        }
    }

    function cameraKey(recorder, channel) {
        return recorder + ":" + channel
    }

    function setFolded(id, folded) {
        const others = foldedRecorders.filter(r => r !== id)
        foldedRecorders = folded ? others.concat(id) : others
    }

    // Each recorder of the side bar as text, so a row is only rebuilt when that recorder changed,
    // and not all of them whenever one connects.
    ListModel { id: recorderModel }

    function syncRecorders(recorders) {
        recorders.forEach((recorder, i) => {
            const json = JSON.stringify(recorder)
            let at = i
            while (at < recorderModel.count && recorderModel.get(at).recorderId !== recorder.id)
                at++
            if (at === recorderModel.count) {
                recorderModel.insert(i, { recorderId: recorder.id, json: json })
                return
            }
            if (at !== i)
                recorderModel.move(at, i, 1)
            if (recorderModel.get(i).json !== json)
                recorderModel.setProperty(i, "json", json)
        })
        if (recorderModel.count > recorders.length)
            recorderModel.remove(recorders.length, recorderModel.count - recorders.length)
    }

    function syncTree() {
        const recorders = JSON.parse(grid.tree || "[]")
        // The button to leave the recordings goes away with the last recorder.
        if (recorders.length === 0)
            setMode("live")
        syncRecorders(recorders)
        const keys = new Set()
        for (const recorder of recorders) {
            for (const camera of recorder.cameras)
                keys.add(cameraKey(recorder.id, camera.number))
        }
        // The recorder was removed, edited or refused the password, so these streams are out of date.
        playbackCameras.forEach((camera, slot) => {
            if (camera && !keys.has(cameraKey(camera.recorder, camera.number)))
                playback.removeCamera(slot)
        })
        syncTiles(recorders)
    }

    // A model changed item by item, so a recorder answering doesn't restart the others' cameras.
    // It reads the layout and not gridKeys, which may not have followed it yet when this runs.
    function syncTiles(recorders) {
        const layout = grid.layout ? JSON.parse(grid.layout) : { cameras: [] }
        const onGrid = new Set(layout.cameras.filter(c => c).map(c => cameraKey(c.recorder, c.channel)))
        const cameras = new Map()
        for (const recorder of recorders) {
            for (const camera of recorder.cameras) {
                const key = cameraKey(recorder.id, camera.number)
                if (onGrid.has(key))
                    cameras.set(key, { key: key, recorder: recorder.id, number: camera.number, pictureAspect: camera.aspect, label: camera.name })
            }
        }
        const keys = new Set(cameras.keys())
        for (let i = tileModel.count - 1; i >= 0; i--) {
            const key = tileModel.get(i).key
            if (cameras.has(key))
                cameras.delete(key)
            else
                tileModel.remove(i)
        }
        for (const camera of cameras.values())
            tileModel.append(camera)
        if (!keys.has(zoomed))
            zoomed = ""
        updateLiveCount()
    }

    GraphicsSetup { id: graphicsSetup }
    HdrPass {
        parent: Overlay.overlay
        anchors.fill: parent
        z: 1000000
    }
    Component.onCompleted: {
        graphicsSetup.prepare(win)
        win.showMaximized()
    }

    function setMode(next) {
        if (next === mode)
            return
        // In this order, or the cameras hidden by the zoom would reopen for an instant.
        mode = next
        zoomed = ""
        sidebarFloating = !playbackCamera
    }

    function isPlayback(recorder, number) {
        return playbackCameras.some(c => c && c.recorder === recorder && c.number === number)
    }

    // Replaces whatever is in the recordings.
    // From a camera watched live, which wants what it recorded today and not the day that was
    // last open. Adding a camera to open recordings keeps their day, so they play together.
    function openPlayback(camera) {
        playback.clear()
        playback.date = ""
        playback.requestedDate = ""
        addPlayback(camera)
    }

    function addPlayback(camera) {
        if (isPlayback(camera.recorder, camera.number))
            return
        const slot = playbackCameras.indexOf(null)
        if (slot < 0) {
            grid.notice = "Up to 4 cameras in the recordings. Remove one to add another."
            return
        }
        grid.notice = ""
        const cameras = playbackCameras.slice()
        cameras[slot] = camera
        playbackCameras = cameras
        sidebarFloating = false
        playback.searchCamera(camera, playback.viewDate)
    }

    function updateLiveCount() {
        let live = 0
        const status = {}
        for (let i = 0; i < cameraTiles.count; i++) {
            const tile = cameraTiles.itemAt(i)
            if (!tile || !tile.included)
                continue
            if (tile.hasVideo)
                live++
            status[tile.key] = tile.hasVideo ? "live" : tile.denied ? "denied"
                : tile.signalLost ? "lost" : "connecting"
        }
        liveCount = live
        cameraStatus = status
    }

    function moveDrag(position) {
        const point = win.contentItem.mapFromItem(null, position.x, position.y)
        dragLabel.x = point.x - dragLabel.width / 2
        dragLabel.y = point.y - dragLabel.height / 2
    }

    function beginDrag(camera, position) {
        moveDrag(position)
        dragged = camera
    }

    function endDrag() {
        if (dragLabel.Drag.target)
            dragLabel.Drag.drop()
        dragged = null
    }

    Shortcut {
        sequence: "Esc"
        enabled: win.zoomed !== "" || win.fullScreen
        onActivated: {
            if (win.zoomed !== "")
                win.zoomed = ""
            else
                win.toggleFullScreen()
        }
    }

    // Without this, Space presses whatever button was clicked last. Text fields keep it, like the
    // arrows, so they work in the search box as usual.
    Shortcut {
        sequence: "Space"
        enabled: win.playerKeys
        onActivated: playback.togglePlaying()
    }

    // The gamma slider keeps the focus after it is used and moves with the arrows, so only these
    // two give way to it. The other keys go on working.
    Shortcut {
        sequence: "Left"
        enabled: win.playerKeys && !win.gammaFocused
        onActivated: playback.skipBack()
    }

    Shortcut {
        sequence: "Right"
        enabled: win.playerKeys && !win.gammaFocused
        onActivated: playback.seek(playback.position + 10)
    }

    Shortcut {
        sequence: "Ctrl+Left"
        enabled: win.playerKeys
        onActivated: playback.previousEvent()
    }

    Shortcut {
        sequence: "Ctrl+Right"
        enabled: win.playerKeys
        onActivated: playback.nextEvent()
    }

    Shortcut {
        sequence: "Ctrl+S"
        enabled: (win.zoomed !== "" || win.inRecordings) && !win.popupOpen
        onActivated: {
            if (win.inRecordings)
                playback.savePhotos()
            else
                win.photoRequested()
        }
    }
    Shortcut {
        sequence: "Ctrl+PgDown"
        enabled: win.layoutKeys
        onActivated: win.stepLayout(1)
    }
    Shortcut {
        sequence: "Ctrl+PgUp"
        enabled: win.layoutKeys
        onActivated: win.stepLayout(-1)
    }

    // Shows the next grid on its own, for a screen that only watches. It waits while a camera is
    // shown alone, the recordings are open, a camera is being dragged or something else has the
    // keys, and a grid chosen by hand gets the whole interval.
    Timer {
        id: cycleTimer
        interval: grid.cycleSeconds * 1000
        repeat: true
        running: grid.cycleSeconds > 0 && win.layoutKeys && !win.minimized
        onTriggered: grid.turnLayout()
    }
    Connections {
        target: grid
        function onCurrentLayoutChanged() {
            if (cycleTimer.running)
                cycleTimer.restart()
        }
    }

    Shortcut {
        sequence: "F11"
        onActivated: win.toggleFullScreen()
    }

    header: Rectangle {
        visible: win.zoomed === "" && !win.fullScreen
        height: visible ? 56 : 0
        color: colors.bar

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: colors.line
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 14
            spacing: 12

            IconButton {
                id: sidebarButton
                visible: win.recorderList.length > 0
                icon.source: "qrc:/rigelcam/icons/sidebar.svg"
                text: sidebar.open ? qsTr("Hide cameras") : qsTr("Show cameras")
                onClicked: {
                    if (win.narrowPlayer)
                        win.sidebarFloating = !win.sidebarFloating
                    else
                        grid.saveSidebarCollapsed(!grid.sidebarCollapsed)
                }
            }

            // Only the icon in a narrow window, where the whole bar wouldn't fit.
            Image {
                source: win.narrowBar ? "qrc:/rigelcam/icon-64.png" : colors.logo
                fillMode: Image.PreserveAspectFit
                Layout.preferredHeight: 30
                Layout.preferredWidth: win.narrowBar ? 30 : 130
                Layout.rightMargin: win.narrowBar ? 4 : 12
                sourceSize.height: 60
            }

            // Both modes side by side, so the one open is plain to see.
            Rectangle {
                visible: win.recorderList.length > 0
                implicitWidth: modeRow.implicitWidth + 4
                implicitHeight: 34
                radius: 4
                color: colors.surface
                Row {
                    id: modeRow
                    anchors.centerIn: parent
                    spacing: 2
                    Repeater {
                        model: [{ mode: "live", label: qsTr("Live") }, { mode: "recordings", label: qsTr("Recordings") }]
                        AppButton {
                            required property var modelData
                            readonly property bool current: win.mode === modelData.mode
                            text: modelData.label
                            implicitHeight: 30
                            fill: current ? colors.raised : "transparent"
                            contentColor: current ? colors.text : colors.muted
                            onClicked: win.setMode(modelData.mode)
                        }
                    }
                }
            }

            Rectangle {
                visible: win.recorderList.length > 0 && !win.inRecordings
                implicitWidth: 7
                implicitHeight: 7
                radius: 4
                color: win.liveCount > 0 ? colors.live : colors.muted
                Layout.leftMargin: 4
            }
            Label {
                // The dot before it still tells whether cameras are live.
                visible: win.recorderList.length > 0 && !win.inRecordings && !win.narrowBar
                font.pixelSize: 13
                // Digits of one width, so the text doesn't shift as cameras come and go.
                font.features: { "tnum": 1 }
                color: colors.muted
                text: qsTr("%1 of %2 live").arg(win.liveCount).arg(win.includedCount)
            }

            // With cameras on screen the centered message below is covered.
            Label {
                visible: win.recorderList.length > 0 && text !== ""
                Layout.leftMargin: 8
                Layout.fillWidth: true
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pixelSize: 13
                color: grid.error !== "" || grid.videoUnavailable ? colors.danger : colors.waiting
                text: win.msg(grid.error || grid.missing || grid.notice)
            }

            Item {
                visible: win.recorderList.length === 0 || (grid.error === "" && grid.missing === "" && grid.notice === "")
                Layout.fillWidth: true
            }

            AppComboBox {
                id: layoutBox
                flat: true
                Layout.preferredWidth: 170
                visible: win.recorderList.length > 0 && !win.inRecordings && win.layoutNames.length > 0
                model: win.layoutNames.map((item, index) => ({ index: index, name: item.name }))
                // A grid deleted can leave the row pointing past the list for a moment.
                labelOf: item => item ? win.layoutLabel(item.index, item.name) : ""
                // Set by hand, since a new list of grids would move the row back to the first one.
                onModelChanged: currentIndex = grid.currentLayout
                Connections {
                    target: grid
                    function onCurrentLayoutChanged() {
                        layoutBox.currentIndex = grid.currentLayout
                    }
                }
                onActivated: index => grid.selectLayout(index)
            }

            IconButton {
                id: newLayoutButton
                visible: layoutBox.visible
                icon.source: "qrc:/rigelcam/icons/plus.svg"
                text: qsTr("New grid")
                onClicked: grid.addLayout()
            }

            IconButton {
                id: layoutButton
                visible: layoutBox.visible
                icon.source: "qrc:/rigelcam/icons/edit.svg"
                text: qsTr("Edit grid")
                onClicked: layoutDialog.openFor(grid.currentLayout)
            }

            IconButton {
                id: namesButton
                visible: win.recorderList.length > 0 && !win.inRecordings
                icon.source: "qrc:/rigelcam/icons/names.svg"
                text: grid.cameraNamesHidden ? qsTr("Show camera names") : qsTr("Hide camera names")
                // Lit while the names show, like the size of the grid in use.
                fill: grid.cameraNamesHidden ? "transparent" : colors.raised
                contentColor: grid.cameraNamesHidden ? colors.muted : colors.text
                onClicked: grid.saveCameraNamesHidden(!grid.cameraNamesHidden)
            }

            // One button per grid size, each drawn as its grid.
            Row {
                visible: win.recorderList.length > 0 && !win.inRecordings
                Repeater {
                    model: 6
                    AppButton {
                        id: sizeButton
                        required property int index
                        readonly property int columns: index + 1
                        readonly property bool current: win.cols === columns
                        // Whole pixels for every size, so the small squares stay sharp.
                        readonly property int cell: [14, 6, 4, 3, 2, 2][index]
                        text: columns === 1 ? qsTr("1 square") : qsTr("%1 squares").arg(columns * columns)
                        implicitWidth: 30
                        leftPadding: 0
                        rightPadding: 0
                        fill: current ? colors.raised : "transparent"
                        onClicked: grid.setGridColumns(columns)
                        contentItem: Item {
                            Grid {
                                anchors.centerIn: parent
                                columns: sizeButton.columns
                                spacing: sizeButton.columns === 2 ? 2 : 1
                                Repeater {
                                    model: sizeButton.columns * sizeButton.columns
                                    Rectangle {
                                        width: sizeButton.cell
                                        height: sizeButton.cell
                                        color: sizeButton.current ? colors.text : colors.muted
                                    }
                                }
                            }
                        }
                        AppToolTip {
                            visible: sizeButton.hovered
                            text: sizeButton.text
                        }
                    }
                }
            }

            // What is seldom used, out of the way of the controls of the grid.
            IconButton {
                id: moreButton
                icon.source: "qrc:/rigelcam/icons/more.svg"
                text: qsTr("More")
                // A press on its own button doesn't close a menu, so the button does it.
                onClicked: moreMenu.visible ? moreMenu.close() : moreMenu.open()
                AppMenu {
                    id: moreMenu
                    x: moreButton.width - width
                    y: moreButton.height + 4
                    AppMenuItem {
                        icon.source: "qrc:/rigelcam/icons/info.svg"
                        text: qsTr("About Rigelcam")
                        onTriggered: aboutDialog.open()
                    }
                    AppMenuItem {
                        icon.source: colors.dark ? "qrc:/rigelcam/icons/sun.svg" : "qrc:/rigelcam/icons/moon.svg"
                        text: colors.dark ? qsTr("Light theme") : qsTr("Dark theme")
                        onTriggered: grid.saveTheme(colors.dark ? "light" : "dark")
                    }
                    AppMenuItem {
                        icon.source: "qrc:/rigelcam/icons/fullscreen.svg"
                        text: qsTr("Full screen (F11)")
                        onTriggered: win.toggleFullScreen()
                    }
                    MenuSeparator {
                        contentItem: Rectangle {
                            implicitHeight: 1
                            color: colors.line
                        }
                    }
                    // In their own names, so someone who can't read the current one finds theirs.
                    AppMenuItem {
                        text: "Português"
                        // An empty icon on the others, so the names line up.
                        icon.source: grid.language === "pt_BR" ? "qrc:/rigelcam/icons/check.svg" : "qrc:/rigelcam/icons/blank.svg"
                        onTriggered: grid.saveLanguage("pt_BR")
                    }
                    AppMenuItem {
                        text: "English"
                        icon.source: grid.language === "en" ? "qrc:/rigelcam/icons/check.svg" : "qrc:/rigelcam/icons/blank.svg"
                        onTriggered: grid.saveLanguage("en")
                    }
                    AppMenuItem {
                        text: "Español"
                        icon.source: grid.language === "es" ? "qrc:/rigelcam/icons/check.svg" : "qrc:/rigelcam/icons/blank.svg"
                        onTriggered: grid.saveLanguage("es")
                    }
                }
                tipVisible: hovered && !moreMenu.visible
            }
        }
    }

    AppDialog {
        id: aboutDialog
        readonly property string repository: "https://github.com/eduardoghi/rigelcam"
        title: qsTr("About Rigelcam")
        // The logo already carries the name.
        header: null
        topPadding: 24
        width: Math.min(400, parent.width - 32)
        ColumnLayout {
            anchors.fill: parent
            spacing: 14
            Image {
                source: colors.logo
                fillMode: Image.PreserveAspectFit
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredHeight: 48
                Layout.preferredWidth: 211
                sourceSize.height: 96
            }
            Label {
                Layout.alignment: Qt.AlignHCenter
                textFormat: Text.PlainText
                font.pixelSize: 14
                font.weight: Font.Medium
                color: colors.text
                text: qsTr("Version %1").arg(grid.appVersion)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: 13
                color: colors.muted
                text: qsTr("Live cameras and recordings from Intelbras and Dahua recorders.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: 13
                color: colors.muted
                text: qsTr("Open source under the MIT license. An independent project, not affiliated with Intelbras or Dahua.")
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 4
                spacing: 8
                AppButton { text: qsTr("Close"); onClicked: aboutDialog.close() }
                AppButton {
                    text: qsTr("What's new")
                    onClicked: Qt.openUrlExternally(aboutDialog.repository + "/releases")
                }
                PrimaryButton {
                    text: "GitHub"
                    onClicked: Qt.openUrlExternally(aboutDialog.repository)
                }
            }
        }
    }

    AppDialog {
        id: addDialog
        // The recorder being edited, or null when adding one.
        property var editing: null
        property bool advanced: false
        title: editing ? qsTr("Edit recorder") : qsTr("Add recorder")
        width: Math.min(440, parent.width - 32)
        closePolicy: grid.formBusy ? Popup.NoAutoClose : Popup.CloseOnEscape
        onOpened: { hostField.forceActiveFocus(); grid.formError = "" }
        onClosed: passwordField.text = ""

        readonly property bool complete: hostField.text.trim() !== "" && userField.text !== "" && (editing !== null || passwordField.text !== "")

        function openFor(recorder) {
            editing = recorder
            hostField.text = recorder ? recorder.host : ""
            nameField.text = recorder ? recorder.name : ""
            userField.text = recorder ? recorder.user : ""
            httpField.text = recorder ? recorder.httpPort : 80
            rtspField.text = recorder ? recorder.rtspPort : 554
            advanced = recorder !== null && (recorder.httpPort !== 80 || recorder.rtspPort !== 554)
            passwordField.shown = false
            open()
        }

        function submit() {
            if (!complete || grid.formBusy)
                return
            const http = parseInt(httpField.text) || 0
            const rtsp = parseInt(rtspField.text) || 0
            if (editing)
                grid.updateRecorder(editing.id, nameField.text, hostField.text, http, rtsp, userField.text, passwordField.text)
            else
                grid.addRecorder(nameField.text, hostField.text, http, rtsp, userField.text, passwordField.text)
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 4
            FormLabel { text: qsTr("Address") }
            FormField {
                id: hostField
                placeholderText: "192.168.0.10"
                onAccepted: addDialog.submit()
                // Shows the port someone typed along with the address where it ends up.
                onEditingFinished: {
                    const port = text.trim().match(/^(?:https?:\/\/)?[^\/:@\s]+:(\d+)(?:\/.*)?$/)
                    if (port)
                        httpField.text = port[1]
                }
            }
            FormLabel { text: qsTr("Name (optional)") }
            FormField { id: nameField; placeholderText: qsTr("Uses the recorder's model"); onAccepted: addDialog.submit() }
            FormLabel { text: qsTr("User") }
            FormField { id: userField; onAccepted: addDialog.submit() }
            FormLabel { text: qsTr("Password") }
            FormField {
                id: passwordField
                // A wrong password tried a few times can lock the user out of the recorder, so it
                // can be checked before saving.
                property bool shown: false
                echoMode: shown ? TextInput.Normal : TextInput.Password
                rightPadding: 40
                placeholderText: addDialog.editing ? qsTr("Blank to keep the current one") : ""
                onAccepted: addDialog.submit()
                AppButton {
                    id: revealButton
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.rightMargin: 4
                    display: AbstractButton.IconOnly
                    icon.source: passwordField.shown ? "qrc:/rigelcam/icons/eye-off.svg" : "qrc:/rigelcam/icons/eye.svg"
                    text: passwordField.shown ? qsTr("Hide password") : qsTr("Show password")
                    implicitWidth: 30
                    implicitHeight: 30
                    leftPadding: 7
                    rightPadding: 7
                    contentColor: colors.muted
                    focusPolicy: Qt.NoFocus
                    onClicked: passwordField.shown = !passwordField.shown
                    AppToolTip {
                        visible: revealButton.hovered
                        text: revealButton.text
                    }
                }
            }
            AppButton {
                Layout.topMargin: 6
                Layout.leftMargin: -12
                implicitHeight: 28
                icon.source: "qrc:/rigelcam/icons/chevron.svg"
                icon.width: 14
                icon.height: 14
                text: qsTr("Advanced")
                onClicked: addDialog.advanced = !addDialog.advanced
            }
            RowLayout {
                visible: addDialog.advanced
                spacing: 8
                ColumnLayout {
                    spacing: 4
                    FormLabel { text: qsTr("HTTP port"); Layout.topMargin: 0 }
                    FormField {
                        id: httpField
                        validator: IntValidator { bottom: 1; top: 65535 }
                        onAccepted: addDialog.submit()
                    }
                }
                ColumnLayout {
                    spacing: 4
                    FormLabel { text: qsTr("RTSP port"); Layout.topMargin: 0 }
                    FormField {
                        id: rtspField
                        validator: IntValidator { bottom: 1; top: 65535 }
                        onAccepted: addDialog.submit()
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 10
                wrapMode: Text.WordWrap
                font.pixelSize: 12
                color: colors.muted
                text: qsTr("Prefer a view-only user over admin. Video travels unencrypted on the network.")
            }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 6
                visible: grid.formError !== ""
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 13
                color: colors.danger
                text: win.msg(grid.formError)
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Layout.topMargin: 14
                spacing: 8
                AppButton { text: qsTr("Cancel"); enabled: !grid.formBusy; onClicked: addDialog.close() }
                PrimaryButton {
                    text: grid.formBusy ? qsTr("Testing…") : qsTr("Test and save")
                    enabled: !grid.formBusy && addDialog.complete
                    onClicked: addDialog.submit()
                }
            }
        }

        // The form closes when saving ends without an error.
        Connections {
            target: grid
            function onFormBusyChanged() {
                if (!grid.formBusy && grid.formError === "" && addDialog.opened) {
                    addDialog.close()
                }
            }
        }
    }

    AppDialog {
        id: exportDialog
        readonly property var cameras: win.playbackCameras.filter(camera => camera)
        readonly property int startTime: win.secondsOf(startField.text)
        readonly property int endTime: win.secondsOf(endField.text)
        readonly property bool valid: startTime >= 0 && endTime > startTime
        title: qsTr("Export clip")
        width: Math.min(400, parent.width - 32)
        onOpened: startField.forceActiveFocus()

        function openAt(seconds) {
            const start = Math.floor(seconds)
            startField.text = playback.clockText(start)
            endField.text = playback.clockText(Math.min(start + 60, 86399))
            open()
        }

        function submit() {
            if (!valid || grid.updateRunning)
                return
            for (const camera of cameras)
                grid.exportClip(camera.recorder, camera.number, camera.name, playback.viewDate, startTime, endTime)
            close()
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                // The names come from the recorder, so they can't be read as markup.
                textFormat: Text.PlainText
                font.pixelSize: 14
                color: colors.text
                text: exportDialog.cameras.map(camera => camera.name).join(", ")
            }
            RowLayout {
                spacing: 12
                ColumnLayout {
                    spacing: 4
                    FormLabel { text: qsTr("Start") }
                    FormField {
                        id: startField
                        inputMask: "99:99:99"
                        inputMethodHints: Qt.ImhDigitsOnly
                        onAccepted: exportDialog.submit()
                    }
                }
                ColumnLayout {
                    spacing: 4
                    FormLabel { text: qsTr("End") }
                    FormField {
                        id: endField
                        inputMask: "99:99:99"
                        inputMethodHints: Qt.ImhDigitsOnly
                        onAccepted: exportDialog.submit()
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 6
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 12
                color: exportDialog.valid ? colors.muted : colors.danger
                text: exportDialog.valid ? qsTr("One video per camera, in the Rigelcam folder of your videos.")
                    : qsTr("The end has to be after the start.")
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Layout.topMargin: 12
                spacing: 8
                AppButton { text: qsTr("Cancel"); onClicked: exportDialog.close() }
                PrimaryButton {
                    // The app closes when the installer of an update starts, which would cut
                    // an export begun during the download.
                    enabled: exportDialog.valid && !grid.updateRunning
                    text: qsTr("Export")
                    onClicked: exportDialog.submit()
                }
            }
        }
    }

    AppDialog {
        id: layoutDialog
        property int index: 0
        title: qsTr("Grid")
        width: Math.min(400, parent.width - 32)

        onOpened: layoutNameField.forceActiveFocus()

        function openFor(index) {
            layoutDialog.index = index
            layoutNameField.text = win.layoutNames[index]?.name ?? ""
            cycleBox.currentIndex = Math.max(0, cycleBox.model.findIndex(c => c.seconds === grid.cycleSeconds))
            open()
        }

        function submit() {
            grid.renameLayout(index, layoutNameField.text)
            grid.saveCycleSeconds(cycleBox.currentValue)
            close()
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 4
            FormLabel { text: qsTr("Name") }
            FormField {
                id: layoutNameField
                placeholderText: win.layoutLabel(layoutDialog.index, "")
                maximumLength: 60
                onAccepted: layoutDialog.submit()
            }
            FormLabel { text: qsTr("Show the next grid") }
            AppComboBox {
                id: cycleBox
                Layout.fillWidth: true
                model: [0, 10, 30, 60].map(seconds => ({ seconds: seconds }))
                valueRole: "seconds"
                labelOf: item => item.seconds === 0 ? qsTr("Never")
                    : item.seconds === 60 ? qsTr("Every minute") : qsTr("Every %1 seconds").arg(item.seconds)
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 14
                spacing: 8
                AppButton {
                    visible: win.layoutNames.length > 1
                    text: qsTr("Delete grid")
                    contentColor: colors.danger
                    onClicked: {
                        layoutDialog.close()
                        removeLayoutDialog.openFor(layoutDialog.index)
                    }
                }
                Item { Layout.fillWidth: true }
                AppButton { text: qsTr("Cancel"); onClicked: layoutDialog.close() }
                PrimaryButton { text: qsTr("Save"); onClicked: layoutDialog.submit() }
            }
        }
    }

    ConfirmDialog {
        id: removeLayoutDialog
        property int index: 0
        title: qsTr("Delete grid")
        message: qsTr("Really delete %1? Its cameras stay in the sidebar.")
            .arg(win.layoutLabel(index, win.layoutNames[index]?.name ?? ""))
        onConfirmed: grid.removeLayout(index)

        function openFor(index) {
            removeLayoutDialog.index = index
            open()
        }
    }

    ConfirmDialog {
        id: removeDialog
        property var recorder: null
        title: qsTr("Delete recorder")
        message: qsTr("Really delete %1? The saved password will be erased, and the recordings on the device stay as they are.").arg(recorder ? recorder.name : "")
        onConfirmed: grid.removeRecorder(recorder.id)
    }

    Column {
        anchors.centerIn: parent
        spacing: 14
        visible: win.recorderList.length === 0
        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error === ""
            // The bright star of the dark theme would disappear on a light page.
            source: colors.dark ? "qrc:/rigelcam/star.svg" : "qrc:/rigelcam/icon-256.png"
            sourceSize.width: 72
            sourceSize.height: 72
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error === ""
            font.pixelSize: 18
            font.weight: Font.DemiBold
            color: colors.text
            text: qsTr("No recorders yet")
        }
        PrimaryButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.ready
            icon.source: "qrc:/rigelcam/icons/plus.svg"
            text: qsTr("Add recorder")
            onClicked: addDialog.openFor(null)
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.error !== ""
            textFormat: Text.PlainText
            font.pixelSize: 14
            color: colors.danger
            text: win.msg(grid.error)
        }
        // Only on a click, since the file may still be recoverable.
        AppButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.listCorrupted
            text: qsTr("Start over")
            edge: colors.line
            onClicked: grid.startOver()
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.missing !== ""
            textFormat: Text.PlainText
            font.pixelSize: 13
            color: grid.videoUnavailable ? colors.danger : colors.waiting
            text: win.msg(grid.missing)
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: grid.notice !== ""
            textFormat: Text.PlainText
            font.pixelSize: 13
            color: colors.waiting
            text: win.msg(grid.notice)
        }
    }

    Rectangle {
        id: sidebar
        readonly property bool open: win.recorderList.length > 0 && win.zoomed === "" && !win.fullScreen
            && (win.narrowPlayer ? win.sidebarFloating : !grid.sidebarCollapsed)
        z: 2
        readonly property string search: cameraSearch.text.trim().toLowerCase()
        visible: open
        width: open ? 260 : 0
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        color: colors.bar

        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 1
            color: colors.line
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 10
            spacing: 8

            RowLayout {
                Layout.leftMargin: 6
                Label {
                    Layout.fillWidth: true
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    color: colors.text
                    text: qsTr("Recorders")
                }
                AppButton {
                    id: addButton
                    display: AbstractButton.IconOnly
                    icon.source: "qrc:/rigelcam/icons/plus.svg"
                    text: qsTr("Add recorder")
                    implicitWidth: 30
                    implicitHeight: 30
                    leftPadding: 7
                    rightPadding: 7
                    onClicked: addDialog.openFor(null)
                    AppToolTip {
                        visible: addButton.hovered
                        text: addButton.text
                    }
                }
            }

            FormField {
                id: cameraSearch
                implicitHeight: 34
                font.pixelSize: 13
                placeholderText: qsTr("Search camera")
            }

            Flickable {
                id: tree
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentHeight: treeColumn.implicitHeight
                // The mouse drags cameras here, and the wheel and the scroll bar move the list.
                acceptedButtons: Qt.NoButton
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}

                Column {
                    id: treeColumn
                    width: tree.width

                    Repeater {
                        model: recorderModel

                        Column {
                            id: recorderGroup
                            required property string json
                            readonly property var modelData: JSON.parse(json)
                            readonly property var matches: modelData.cameras.filter(c => c.name.toLowerCase().includes(sidebar.search))
                            readonly property bool folded: sidebar.search === "" && win.foldedRecorders.includes(modelData.id)
                            visible: sidebar.search === "" || matches.length > 0
                            width: treeColumn.width

                            Rectangle {
                                width: parent.width
                                height: 34
                                radius: 3
                                color: recorderHover.hovered ? colors.raised : "transparent"

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 4
                                    anchors.rightMargin: 4
                                    spacing: 6
                                    Image {
                                        source: "qrc:/rigelcam/icons/chevron.svg"
                                        sourceSize.width: 16
                                        sourceSize.height: 16
                                        rotation: recorderGroup.folded ? -90 : 0
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: recorderGroup.modelData.name
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        font.pixelSize: 13
                                        font.weight: Font.DemiBold
                                        color: colors.text
                                    }
                                    AppButton {
                                        id: editButton
                                        visible: recorderHover.hovered
                                        display: AbstractButton.IconOnly
                                        icon.source: "qrc:/rigelcam/icons/edit.svg"
                                        icon.width: 14
                                        icon.height: 14
                                        text: qsTr("Edit recorder")
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        leftPadding: 6
                                        rightPadding: 6
                                        onClicked: addDialog.openFor(recorderGroup.modelData)
                                        AppToolTip {
                                            visible: editButton.hovered
                                            text: editButton.text
                                        }
                                    }
                                    AppButton {
                                        id: removeButton
                                        visible: recorderHover.hovered
                                        display: AbstractButton.IconOnly
                                        icon.source: "qrc:/rigelcam/icons/trash.svg"
                                        icon.width: 14
                                        icon.height: 14
                                        text: qsTr("Remove recorder")
                                        implicitWidth: 26
                                        implicitHeight: 26
                                        leftPadding: 6
                                        rightPadding: 6
                                        onClicked: {
                                            removeDialog.recorder = recorderGroup.modelData
                                            removeDialog.open()
                                        }
                                        AppToolTip {
                                            visible: removeButton.hovered
                                            text: removeButton.text
                                        }
                                    }
                                }

                                HoverHandler { id: recorderHover }
                                // Folding waits for a second click, so a double click only opens the cameras.
                                Timer {
                                    id: foldLater
                                    interval: Qt.styleHints.mouseDoubleClickInterval
                                    onTriggered: win.setFolded(recorderGroup.modelData.id, !recorderGroup.folded)
                                }
                                TapHandler {
                                    onTapped: if (tapCount === 1) foldLater.restart()
                                    onDoubleTapped: {
                                        foldLater.stop()
                                        if (!win.inRecordings)
                                            grid.openRecorder(recorderGroup.modelData.id)
                                    }
                                }
                            }

                            Text {
                                visible: recorderGroup.modelData.state === "connecting"
                                leftPadding: 26
                                bottomPadding: 6
                                font.pixelSize: 12
                                color: colors.muted
                                text: qsTr("Connecting…")
                            }

                            Column {
                                visible: recorderGroup.modelData.state === "failed"
                                width: parent.width
                                leftPadding: 26
                                bottomPadding: 6
                                spacing: 2
                                Text {
                                    width: parent.width - parent.leftPadding
                                    wrapMode: Text.WordWrap
                                    textFormat: Text.PlainText
                                    font.pixelSize: 12
                                    color: colors.danger
                                    text: win.msg(recorderGroup.modelData.failure || "")
                                }
                                AppButton {
                                    text: qsTr("Try again")
                                    implicitHeight: 28
                                    leftPadding: 8
                                    rightPadding: 8
                                    onClicked: grid.retryRecorder(recorderGroup.modelData.id)
                                }
                            }

                            // The rows stay while the search changes and only hide, since rebuilding
                            // all of them on every key costs about ten objects a camera.
                            Repeater {
                                model: recorderGroup.folded ? [] : recorderGroup.modelData.cameras

                                Rectangle {
                                    id: cameraRow
                                    required property var modelData
                                    visible: sidebar.search === "" || modelData.name.toLowerCase().includes(sidebar.search)
                                    readonly property string key: win.cameraKey(recorderGroup.modelData.id, modelData.number)
                                    readonly property bool onGrid: win.gridKeys.includes(key)
                                    readonly property bool isPlayback: win.inRecordings
                                        && win.isPlayback(recorderGroup.modelData.id, modelData.number)
                                    // The live cameras are stopped in the recordings, so their state means nothing.
                                    readonly property string status: win.inRecordings ? "" : (win.cameraStatus[key] ?? "")
                                    readonly property var payload: ({
                                        recorder: recorderGroup.modelData.id,
                                        number: modelData.number,
                                        name: modelData.name
                                    })
                                    width: recorderGroup.width
                                    height: 30
                                    radius: 3
                                    color: rowHover.hovered ? colors.raised : "transparent"

                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 26
                                        anchors.rightMargin: 8
                                        spacing: 9
                                        Rectangle {
                                            implicitWidth: 8
                                            implicitHeight: 8
                                            radius: 4
                                            color: cameraRow.isPlayback ? colors.accent
                                                 : cameraRow.status === "live" ? colors.live
                                                 : cameraRow.status === "lost" || cameraRow.status === "denied" ? colors.danger
                                                 : cameraRow.status === "connecting" ? colors.waiting
                                                 : "transparent"
                                            border.color: cameraRow.onGrid && !win.inRecordings ? color : cameraRow.isPlayback ? colors.accent : colors.muted
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: cameraRow.modelData.name
                                            textFormat: Text.PlainText
                                            elide: Text.ElideRight
                                            font.pixelSize: 13
                                            color: cameraRow.onGrid || cameraRow.isPlayback ? colors.text : colors.muted
                                        }
                                    }

                                    AppToolTip {
                                        visible: rowHover.hovered && !win.inRecordings
                                        text: cameraRow.status === "live" ? qsTr("Live")
                                            : cameraRow.status === "denied" ? qsTr("No permission")
                                            : cameraRow.status === "lost" ? qsTr("No signal")
                                            : cameraRow.status === "connecting" ? qsTr("Connecting…")
                                            : qsTr("Outside the grid")
                                    }

                                    HoverHandler { id: rowHover }
                                    TapHandler {
                                        onDoubleTapped: {
                                            if (win.inRecordings)
                                                win.addPlayback(cameraRow.payload)
                                            else
                                                grid.addCamera(recorderGroup.modelData.id, cameraRow.modelData.number)
                                        }
                                    }
                                    CameraDrag { camera: cameraRow.payload }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Item {
        id: cameraArea
        readonly property real cellWidth: (width - (win.cols - 1) * 2) / win.cols
        readonly property real cellHeight: (height - (win.cols - 1) * 2) / win.cols
        // The tile under the mouse, whose name leaves room for the button that removes it.
        property string hoveredKey: ""
        anchors.left: sidebar.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        visible: !win.inRecordings

        ListModel { id: tileModel }

        // One tile per camera and not per square, so moving a camera doesn't reopen its stream. Only
        // cameras on the grid get one, since a large recorder can list many more than it shows.
        Repeater {
            id: cameraTiles
            model: tileModel

            VideoItem {
                id: tile
                required property string key
                required property int recorder
                required property int number
                required property string label
                required property real pictureAspect
                readonly property var payload: ({ recorder: recorder, number: number, name: label })
                readonly property int square: win.gridKeys.indexOf(key)
                readonly property bool included: square >= 0
                readonly property bool shown: included && !win.inRecordings && !win.minimized && (win.zoomed === "" || win.zoomed === key)
                readonly property bool isMain: win.zoomed === key
                view: isMain ? win.zoom.view : Qt.rect(0, 0, 1, 1)
                aspect: pictureAspect
                gamma: isMain ? win.gamma : 1

                visible: shown
                x: isMain ? 0 : (square % win.cols) * (cameraArea.cellWidth + 2)
                y: isMain ? 0 : Math.floor(square / win.cols) * (cameraArea.cellHeight + 2)
                width: isMain ? cameraArea.width : cameraArea.cellWidth
                height: isMain ? cameraArea.height : cameraArea.cellHeight

                // Once per turn of the event loop, since cameras connecting together each change it.
                onStatusChanged: Qt.callLater(win.updateLiveCount)
                onPhotoFinished: file => photoToast.show(file)
                // Later, because this closes the tile that is emitting the signal.
                onAuthRefused: Qt.callLater(grid.recorderRefused.bind(grid, recorder))
                onIncludedChanged: Qt.callLater(win.updateLiveCount)

                // Hidden cameras keep running for a while, since going back to the grid right after
                // zooming is common and reconnecting all of them takes seconds. Recordings stop them
                // right away, as they need the network and the decoder at the same moment.
                Timer {
                    id: stopHidden
                    interval: 20000
                    onTriggered: grid.show(tile, tile.recorder, tile.number, false, false)
                }
                function apply() {
                    if (shown) {
                        stopHidden.stop()
                        grid.show(tile, recorder, number, true, false)
                    } else if (included && !win.inRecordings) {
                        stopHidden.restart()
                    } else {
                        stopHidden.stop()
                        grid.show(tile, recorder, number, false, false)
                    }
                }
                Component.onCompleted: apply()
                onShownChanged: apply()

                Rectangle {
                    anchors.fill: parent
                    visible: !tile.hasVideo
                    color: colors.surface
                    Column {
                        anchors.centerIn: parent
                        spacing: 6
                        Image {
                            anchors.horizontalCenter: parent.horizontalCenter
                            visible: tile.signalLost
                            source: "qrc:/rigelcam/icons/camera-off.svg"
                            sourceSize.width: 24
                            sourceSize.height: 24
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: tile.denied ? qsTr("No permission")
                                : tile.signalLost ? qsTr("No signal") : qsTr("Connecting…")
                            font.pixelSize: 12
                            color: colors.faint
                        }
                    }
                }

                // The main stream goes over the substream, which keeps running, so zooming in and
                // out never freezes the picture. It appears when its first frame arrives.
                VideoItem {
                    id: mainStream
                    anchors.fill: parent
                    visible: hasVideo && tile.isMain
                    view: tile.view
                    gamma: tile.gamma
                    superResolution: tile.isMain && win.superResolution
                    rtxHdr: tile.isMain && win.rtxHdr
                    onPhotoFinished: file => photoToast.show(file)
                    onAuthRefused: Qt.callLater(grid.recorderRefused.bind(grid, tile.recorder))
                    // Waits like stopHidden, since a window often comes back right after minimizing.
                    Timer {
                        id: stopMain
                        interval: 20000
                        onTriggered: grid.show(mainStream, tile.recorder, tile.number, false, true)
                    }
                    function apply() {
                        if (tile.isMain && tile.shown) {
                            stopMain.stop()
                            grid.show(mainStream, tile.recorder, tile.number, true, true)
                        } else if (tile.isMain && !win.inRecordings) {
                            stopMain.restart()
                        } else {
                            stopMain.stop()
                            grid.show(mainStream, tile.recorder, tile.number, false, true)
                        }
                    }
                    Connections {
                        target: tile
                        function onIsMainChanged() { mainStream.apply() }
                        function onShownChanged() { mainStream.apply() }
                    }
                }

                Connections {
                    target: win
                    enabled: tile.isMain
                    function onPhotoRequested() {
                        const name = tile.label + " " + Qt.formatDateTime(new Date(), "yyyy-MM-dd HH-mm-ss")
                        const shown = mainStream.visible ? mainStream : tile
                        shown.savePhoto(name)
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    visible: tileHover.hovered && !tile.isMain
                    color: "transparent"
                    border.color: colors.accent
                    border.width: 2
                }

                AppButton {
                    id: leaveGrid
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 6
                    visible: tileHover.hovered && !tile.isMain
                    display: AbstractButton.IconOnly
                    icon.source: "qrc:/rigelcam/icons/close.svg"
                    icon.width: 14
                    icon.height: 14
                    text: qsTr("Remove from the grid")
                    implicitWidth: 26
                    implicitHeight: 26
                    leftPadding: 6
                    rightPadding: 6
                    fill: colors.overVideo
                    contentColor: colors.overVideoText
                    onClicked: grid.removeCamera(tile.recorder, tile.number)
                    AppToolTip {
                        visible: leaveGrid.hovered
                        text: leaveGrid.text
                    }
                }

                HoverHandler {
                    id: tileHover
                    onHoveredChanged: {
                        if (hovered)
                            cameraArea.hoveredKey = tile.key
                        else if (cameraArea.hoveredKey === tile.key)
                            cameraArea.hoveredKey = ""
                    }
                }
                ZoomGestures {
                    enabled: tile.isMain
                    zoom: win.zoom
                }
                TapHandler {
                    onDoubleTapped: {
                        win.zoomedName = tile.label
                        win.zoomedCamera = tile.payload
                        win.zoomed = win.zoomed ? "" : tile.key
                    }
                }
                CameraDrag {
                    enabled: !tile.isMain
                    camera: tile.payload
                }
            }
        }

        // The names go above every video and not inside each tile. A video is drawn by a node of its
        // own, which the scene graph can't batch with anything, so names between videos would each
        // be drawn on their own, and together they become one or two draws.
        Repeater {
            model: tileModel

            // At the top because recorders usually print their own title at the bottom left.
            OverlayLabel {
                id: nameTag
                required property string key
                required property string label
                readonly property int square: win.gridKeys.indexOf(key)
                // Hidden names still show under the mouse, to tell which camera a square is.
                visible: square >= 0 && win.zoomed === "" && !win.inRecordings
                    && (!grid.cameraNamesHidden || cameraArea.hoveredKey === key)
                x: (square % win.cols) * (cameraArea.cellWidth + 2) + 6
                y: Math.floor(square / win.cols) * (cameraArea.cellHeight + 2) + 6
                text: label
                maxWidth: cameraArea.cellWidth - (cameraArea.hoveredKey === key ? 44 : 12)
            }
        }

        // A number and not the list of cameras, so a drop doesn't destroy the area handling it.
        Repeater {
            model: win.recorderList.length > 0 ? win.cols * win.cols : 0

            Item {
                id: square
                required property int index
                readonly property var key: win.gridKeys[index] ?? null
                readonly property bool hasTile: key !== null && win.tileKeys.includes(key)
                visible: win.zoomed === "" && !win.inRecordings
                x: (index % win.cols) * (cameraArea.cellWidth + 2)
                y: Math.floor(index / win.cols) * (cameraArea.cellHeight + 2)
                width: cameraArea.cellWidth
                height: cameraArea.cellHeight

                // Only for an empty square. Under a camera nothing of it shows, and a grid of 36 would
                // build 30 or so buttons that never appear.
                Loader {
                    anchors.fill: parent
                    active: !square.hasTile
                    sourceComponent: Component {
                        Rectangle {
                            color: colors.surface

                            // A camera the recorder stopped listing can be gone for good, and only the
                            // user knows that.
                            AppButton {
                                id: clearSquare
                                anchors.right: parent.right
                                anchors.top: parent.top
                                anchors.margins: 6
                                visible: square.key !== null && squareHover.hovered
                                display: AbstractButton.IconOnly
                                icon.source: "qrc:/rigelcam/icons/close.svg"
                                icon.width: 14
                                icon.height: 14
                                text: qsTr("Remove from the grid")
                                implicitWidth: 26
                                implicitHeight: 26
                                leftPadding: 6
                                rightPadding: 6
                                fill: colors.overVideo
                                contentColor: colors.overVideoText
                                onClicked: {
                                    const camera = win.gridLayout.cameras[square.index]
                                    grid.removeCamera(camera.recorder, camera.channel)
                                }
                                AppToolTip {
                                    visible: clearSquare.hovered
                                    text: clearSquare.text
                                }
                            }
                            // An empty square is a faint plus, and says what it takes under the mouse or
                            // while a camera is dragged, so a big grid with few cameras stays quiet.
                            readonly property bool explained: square.key !== null || squareHover.hovered
                                || win.dragged !== null
                            Text {
                                anchors.centerIn: parent
                                visible: !parent.explained
                                text: "+"
                                font.pixelSize: 26
                                font.weight: Font.Light
                                color: colors.faint
                                opacity: 0.6
                            }
                            Text {
                                anchors.centerIn: parent
                                visible: parent.explained
                                width: Math.min(implicitWidth, parent.width - 12)
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignHCenter
                                // A camera of a recorder that is offline or hasn't answered keeps its square.
                                text: square.key === null ? qsTr("Drag a camera here") : qsTr("Waiting for the recorder")
                                font.pixelSize: 12
                                color: colors.faint
                            }
                        }
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    visible: dropArea.containsDrag
                    color: "transparent"
                    border.color: colors.accent
                    border.width: 3
                }

                // Only without a tile, since this layer is above the tiles and would take their hover.
                HoverHandler {
                    id: squareHover
                    enabled: square.key !== null && !square.hasTile
                }

                DropArea {
                    id: dropArea
                    anchors.fill: parent
                    onDropped: grid.placeCamera(win.dragged.recorder, win.dragged.number, square.index)
                }
            }
        }
    }

    Rectangle {
        visible: win.zoomed !== ""
        x: 12
        y: 12
        // The margins of the row inside, and no wider than the window, where the name gives way.
        width: Math.min(Math.ceil(zoomBar.implicitWidth) + 18, parent.width - 24)
        height: 42
        radius: 6
        color: colors.panel
        border.color: colors.line
        RowLayout {
            id: zoomBar
            anchors.fill: parent
            anchors.leftMargin: 4
            anchors.rightMargin: 14
            spacing: 10
            AppButton {
                icon.source: "qrc:/rigelcam/icons/back.svg"
                text: qsTr("Back")
                onClicked: win.zoomed = ""
            }
            AppButton {
                text: qsTr("Recordings")
                onClicked: {
                    const camera = win.zoomedCamera
                    win.setMode("recordings")
                    win.openPlayback(camera)
                }
            }
            Text {
                Layout.fillWidth: true
                Layout.preferredWidth: Math.ceil(implicitWidth)
                Layout.minimumWidth: 40
                text: win.zoomedName
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pixelSize: 13
                font.weight: Font.Medium
                color: colors.text
            }
            IconButton {
                id: zoomPhoto
                icon.source: "qrc:/rigelcam/icons/camera.svg"
                text: qsTr("Save photo")
                onClicked: win.photoRequested()
                tip: text + " (Ctrl+S)"
            }
            IconButton {
                id: zoomOut
                Layout.leftMargin: 8
                enabled: win.zoom.scale > 1
                icon.source: "qrc:/rigelcam/icons/minus.svg"
                text: qsTr("Zoom out")
                onClicked: win.zoom.zoomAt(win.zoom.scale / 1.5, 0.5, 0.5)
            }
            Text {
                text: Math.round(win.zoom.scale * 100) + "%"
                horizontalAlignment: Text.AlignHCenter
                Layout.preferredWidth: 42
                font.pixelSize: 13
                color: colors.text
            }
            IconButton {
                id: zoomIn
                enabled: win.zoom.scale < 8
                icon.source: "qrc:/rigelcam/icons/plus.svg"
                text: qsTr("Zoom in")
                onClicked: win.zoom.zoomAt(win.zoom.scale * 1.5, 0.5, 0.5)
            }
            SuperResolutionButton { Layout.leftMargin: 8 }
            RtxHdrButton {}
            GammaControl { Layout.leftMargin: 8 }
        }
    }

    Item {
        id: playback
        anchors.left: win.narrowPlayer ? parent.left : sidebar.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        visible: win.inRecordings

        // The day shown, as YYYY-MM-DD on the clock of the recorder.
        property string date: ""
        // What each camera recorded on the day, by key, and all of it together for the bar.
        property var spansOf: ({})
        property var spans: []
        // Kept with the spans, since the event buttons ask for them as the position moves.
        readonly property var eventRuns: Playback.eventRuns(spans)
        // The events of the open cameras, by time, for the list of the day. Without
        // Object.fromEntries, which the JavaScript of QML doesn't have.
        readonly property var events: Playback.eventList(spansOf, win.playbackCameras.reduce((names, camera) => {
            if (camera)
                names[win.cameraKey(camera.recorder, camera.number)] = camera.name
            return names
        }, {}))
        // The clock of each recorder, by recorder, as { today, seconds }, since cameras of
        // different recorders can be open together and their clocks don't agree.
        property var clocks: ({})
        // The bar is shared, so the recorder that is behind decides, and no day shows as over while
        // one of them is still in it.
        readonly property string today: Playback.earliestDay(clocks, win.playbackCameras)
        // Seconds of the day shown already past on every recorder, all of it when none is in it.
        readonly property int nowSeconds: Playback.secondsOn(date, clocks, win.playbackCameras)
        property int rate: 1
        property bool playing: false
        // Cameras waiting for their search, and the ones that only bring the day up to date.
        property var pending: ({})
        property var refreshing: ({})
        readonly property bool loading: Object.keys(pending).length > 0
        // Failed searches, by key.
        property var errors: ({})
        // The day asked for, which `date` becomes when the answers come.
        property string requestedDate: ""
        readonly property string viewDate: requestedDate || date
        // Whether the first stream of this day already opened.
        property bool started: false
        // The time on the bar, which comes from the first camera that is playing.
        property real position: 0

        readonly property var recordings: grid.recordings ? JSON.parse(grid.recordings) : null

        function keyOf(camera) {
            return win.cameraKey(camera.recorder, camera.number)
        }

        function eachCell(action) {
            for (let i = 0; i < cells.count; i++) {
                const cell = cells.itemAt(i)
                if (cell && cell.camera)
                    action(cell)
            }
        }

        function savePhotos() {
            for (let i = 0; i < cells.count; i++) {
                const cell = cells.itemAt(i)
                if (cell && cell.camera && cell.streamOpen)
                    cell.savePhoto()
            }
        }

        function leadCell() {
            for (let i = 0; i < cells.count; i++) {
                const cell = cells.itemAt(i)
                if (cell && cell.camera && cell.streamOpen)
                    return cell
            }
            return null
        }

        function updateSpans() {
            spans = Playback.allSpans(spansOf)
        }

        // A search that nobody waits for anymore would leave `loading` on for good.
        function forgetSearch(key) {
            delete refreshing[key]
            const waiting = Object.assign({}, pending)
            delete waiting[key]
            pending = waiting
            if (!loading)
                requestedDate = ""
        }

        function clearError(key) {
            if (errors[key] === undefined)
                return
            const others = Object.assign({}, errors)
            delete others[key]
            errors = others
        }

        function lastRecordedSecond() {
            return Playback.lastRecordedSecond(spans)
        }

        // With every answer in, so the cameras of a day open together.
        function startWhenReady() {
            if (started || loading || spans.length === 0)
                return
            started = true
            seek(Playback.startOfDay(spans, date === today))
        }

        onRecordingsChanged: {
            if (!recordings)
                return
            const key = win.cameraKey(recordings.recorder, recordings.channel)
            const refreshed = refreshing[key] === true
            forgetSearch(key)
            if (!win.inRecordings || !win.isPlayback(recordings.recorder, recordings.channel))
                return
            if (recordings.error) {
                if (!refreshed)
                    errors = Object.assign({}, errors, { [key]: recordings.error })
            } else {
                clearError(key)
                date = recordings.date
                const now = recordings.now.split(" ")
                const clock = now[1].split(":")
                const seconds = parseInt(clock[0]) * 3600 + parseInt(clock[1]) * 60 + parseInt(clock[2])
                clocks = Object.assign({}, clocks, { [recordings.recorder]: { today: now[0], seconds: seconds } })
                // The day of today is asked again every minute, and most answers are the same. A new
                // list would rebuild everything drawn from it, the list of events among them.
                if (JSON.stringify(spansOf[key]) !== JSON.stringify(recordings.spans)) {
                    spansOf = Object.assign({}, spansOf, { [key]: recordings.spans })
                    updateSpans()
                }
            }
            if (started)
                syncCells()
            else
                startWhenReady()
        }

        function searchCamera(camera, day) {
            pending = Object.assign({}, pending, { [keyOf(camera)]: true })
            requestedDate = day
            grid.searchRecordings(camera.recorder, camera.number, day)
        }

        // Nothing of the previous day stays on screen while the answers don't come.
        function resetDay() {
            stop()
            spansOf = {}
            spans = []
            position = 0
            errors = {}
            started = false
            refreshing = {}
            pending = {}
        }

        // The recorder keeps recording, so the current day goes stale on screen.
        function refresh() {
            if (loading || !date)
                return
            for (const camera of win.playbackCameras) {
                if (camera) {
                    refreshing[keyOf(camera)] = true
                    grid.searchRecordings(camera.recorder, camera.number, date)
                }
            }
        }

        function clear() {
            resetDay()
            win.playbackCameras = [null, null, null, null]
        }

        function removeCamera(slot) {
            const camera = win.playbackCameras[slot]
            grid.notice = ""
            cells.itemAt(slot).close()
            const cameras = win.playbackCameras.slice()
            cameras[slot] = null
            win.playbackCameras = cameras
            const key = keyOf(camera)
            const others = Object.assign({}, spansOf)
            delete others[key]
            spansOf = others
            updateSpans()
            clearError(key)
            forgetSearch(key)
            if (!win.playbackCamera)
                clear()
            else
                startWhenReady()
        }

        function changeDay(day) {
            const cameras = win.playbackCameras.filter(camera => camera)
            resetDay()
            for (const camera of cameras)
                searchCamera(camera, day)
        }

        function stepDay(days) {
            // From the day asked for, so a second click doesn't wait for the first answer.
            const day = new Date(viewDate + "T12:00:00")
            day.setDate(day.getDate() + days)
            changeDay(win.isoDay(day))
        }

        function stop() {
            playing = false
            eachCell(cell => cell.close())
        }

        property bool resumeWhenShown: false

        function windowMinimized(minimized) {
            if (minimized && playing && leadCell()) {
                resumeWhenShown = true
                togglePlaying()
            } else if (!minimized && resumeWhenShown) {
                resumeWhenShown = false
                togglePlaying()
            }
        }

        function recordedTime(seconds) {
            return Playback.recordedTime(spans, seconds)
        }

        function skipBack() {
            seek(Playback.skipBackTarget(spans, position))
        }

        function previousEvent() {
            const target = Playback.previousEventIn(eventRuns, position, rate)
            if (target >= 0)
                seek(target)
        }

        function nextEvent() {
            const target = Playback.nextEventIn(eventRuns, position)
            if (target >= 0)
                seek(target)
        }

        function seek(seconds) {
            const target = recordedTime(seconds)
            if (target < 0 || !win.playbackCamera)
                return
            timeline.following = true
            position = target
            let opened = false
            eachCell(cell => {
                cell.finished = false
                cell.resyncWait = cell.firstResyncWait
                cell.nextResync = 0
                if (cell.recordedAt(target))
                    cell.open(target)
                else
                    cell.close()
                opened = opened || cell.streamOpen
            })
            playing = opened
        }

        function setRate(next) {
            rate = next
            seek(position)
        }

        function togglePlaying() {
            if (!leadCell()) {
                // After the last recording it starts the day over.
                seek(position >= lastRecordedSecond() - 2 ? spans[0].start : position)
                return
            }
            playing = !playing
            eachCell(cell => {
                if (cell.streamOpen)
                    grid.pausePlayback(cell.video, !playing)
            })
        }

        // Keeps the cameras on the same moment. Each stream is paced by the recorder on its own, so
        // one that drifts is opened again at the time of the first, and cameras without a
        // recording at this moment close until the bar reaches one.
        function syncCells() {
            if (!playing)
                return
            const lead = leadCell()
            eachCell(cell => {
                if (!cell.recordedAt(position)) {
                    if (cell.streamOpen)
                        cell.close()
                    // Its stream ended at this gap, and the next recording needs another one.
                    cell.finished = false
                } else if (!cell.streamOpen && !cell.failed && !cell.finished) {
                    cell.open(position)
                } else if (cell !== lead && cell.drifted(position)) {
                    cell.resync(position)
                }
            })
            if (leadCell())
                return
            // All of them reached a gap, so the bar jumps to the next recording of any of them.
            let failed = false
            eachCell(cell => failed = failed || cell.failed)
            const next = recordedTime(position + 1)
            if (failed || next < 0)
                playing = false
            else
                seek(next)
        }

        function clockText(seconds) {
            return Playback.clockText(seconds)
        }

        onVisibleChanged: {
            if (!visible) {
                stop()
            } else if (win.playbackCamera && spans.length === 0) {
                const cameras = win.playbackCameras.filter(camera => camera)
                const day = date
                resetDay()
                for (const camera of cameras)
                    searchCamera(camera, day)
            } else {
                refresh()
            }
        }

        Timer {
            interval: 60000
            repeat: true
            running: playback.visible && playback.date !== "" && playback.date === playback.today
            onTriggered: playback.refresh()
        }

        Timer {
            interval: 1000
            repeat: true
            running: playback.visible && playback.playing
            onTriggered: playback.syncCells()
        }

        Item {
            id: videoArea
            // Slots in use, in order, which is how they take their place on screen.
            readonly property var used: [0, 1, 2, 3].filter(slot => win.playbackCameras[slot] !== null)
            readonly property int columns: used.length > 1 ? 2 : 1
            readonly property int rows: used.length > 2 ? 2 : 1
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: controls.top

            Repeater {
                id: cells
                model: 4

                Item {
                    id: cell
                    required property int index
                    readonly property var camera: win.playbackCameras[index]
                    readonly property string key: camera ? playback.keyOf(camera) : ""
                    readonly property int place: videoArea.used.indexOf(index)
                    readonly property var spans: playback.spansOf[key] ?? []
                    readonly property string searchError: playback.errors[key] ?? ""
                    readonly property alias video: video
                    // Stays while the time or the speed changes, to follow one spot of the image.
                    readonly property VideoZoom zoom: VideoZoom {}
                    onKeyChanged: zoom.reset()
                    property int attempt: 0
                    property bool streamOpen: false
                    property bool failed: false
                    // The recording of this camera ended before what the search listed.
                    property bool finished: false
                    // Where its stream opened, where it is now, and whether it already said so.
                    property int openedAt: 0
                    property real position: 0
                    property bool reported: false

                    visible: camera !== null
                    x: (place % videoArea.columns) * width
                    y: Math.floor(place / videoArea.columns) * height
                    width: videoArea.width / videoArea.columns
                    height: videoArea.height / videoArea.rows

                    function recordedAt(seconds) {
                        return Playback.recordedAt(spans, seconds)
                    }

                    function open(seconds) {
                        openedAt = Math.floor(seconds)
                        position = openedAt
                        reported = false
                        failed = false
                        attempt = grid.play(video, camera.recorder, camera.number, playback.date, openedAt, playback.rate)
                        streamOpen = attempt !== 0
                        failed = !streamOpen
                    }

                    function savePhoto() {
                        const time = playback.clockText(position).replace(/:/g, "-")
                        video.savePhoto(camera.name + " " + playback.date + " " + time)
                    }

                    function close() {
                        attempt = 0
                        streamOpen = false
                        failed = false
                        finished = false
                        reported = false
                        grid.show(video, 0, 0, false, false)
                    }

                    // A stream slow to open is behind again when it opens, so reopening at once
                    // would never end, with a login on the recorder each time. It waits longer
                    // after every try, and a camera can stay a few seconds behind meanwhile.
                    readonly property int firstResyncWait: 15000
                    property int resyncWait: firstResyncWait
                    property real nextResync: 0

                    function resync(seconds) {
                        const now = Date.now()
                        if (now < nextResync)
                            return
                        nextResync = now + resyncWait
                        resyncWait = Math.min(resyncWait * 2, 240000)
                        open(seconds)
                    }

                    // A reopened stream starts up to a keyframe earlier, so small gaps are normal.
                    function drifted(seconds) {
                        return streamOpen && reported && Math.abs(position - seconds) > 3 * playback.rate
                    }

                    function timeAfter(elapsed) {
                        return Playback.timeAfter(spans, openedAt, elapsed)
                    }

                    VideoItem {
                        id: video
                        anchors.fill: parent
                        anchors.margins: 1
                        gamma: win.gamma
                        view: cell.zoom.view
                        superResolution: win.superResolution && videoArea.used.length === 1
                        rtxHdr: win.rtxHdr && videoArea.used.length === 1

                        onPhotoFinished: file => photoToast.show(file)
                        onPlaybackPosition: (seconds, attempt) => {
                            if (attempt !== cell.attempt)
                                return
                            cell.reported = true
                            cell.position = cell.timeAfter(seconds)
                            if (playback.leadCell() === cell)
                                playback.position = cell.position
                        }
                        onPlaybackEnded: attempt => {
                            if (attempt !== cell.attempt)
                                return
                            cell.streamOpen = false
                            cell.finished = true
                            playback.syncCells()
                        }
                        onPlaybackFailed: attempt => {
                            if (attempt !== cell.attempt)
                                return
                            cell.streamOpen = false
                            cell.failed = true
                            playback.syncCells()
                        }
                        onAuthRefused: {
                            cell.streamOpen = false
                            cell.failed = true
                            if (cell.camera)
                                Qt.callLater(grid.recorderRefused.bind(grid, cell.camera.recorder))
                        }
                    }

                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 1
                        visible: !cell.streamOpen
                        color: colors.page
                    }

                    Label {
                        anchors.centerIn: parent
                        width: Math.min(implicitWidth, parent.width - 24)
                        visible: cell.failed || cell.searchError !== "" || (playback.started && !cell.streamOpen && !cell.recordedAt(playback.position))
                        padding: 12
                        wrapMode: Text.WordWrap
                        horizontalAlignment: Text.AlignHCenter
                        // Failures can carry text from the recorder.
                        textFormat: Text.PlainText
                        font.pixelSize: 14
                        color: cell.failed || cell.searchError !== "" ? colors.danger : colors.muted
                        text: cell.searchError !== "" ? win.msg(cell.searchError)
                            : video.denied ? qsTr("You don't have permission to see the recordings of this camera.")
                            : cell.failed ? qsTr("Could not open this recording. Try another time on the bar.")
                            : qsTr("No recording at this time.")
                    }

                    ZoomGestures {
                        enabled: video.hasVideo
                        zoom: cell.zoom
                    }

                    OverlayLabel {
                        visible: cell.camera !== null && videoArea.used.length > 1
                        x: 8
                        y: 8
                        text: cell.camera ? cell.camera.name : ""
                        maxWidth: cell.width - 16
                    }

                    AppButton {
                        id: cellPhoto
                        anchors.right: closeCell.left
                        anchors.top: parent.top
                        anchors.topMargin: 8
                        anchors.rightMargin: 4
                        visible: cellHover.hovered && video.hasVideo
                        display: AbstractButton.IconOnly
                        icon.source: "qrc:/rigelcam/icons/camera.svg"
                        icon.width: 14
                        icon.height: 14
                        text: qsTr("Save photo")
                        implicitWidth: 26
                        implicitHeight: 26
                        leftPadding: 6
                        rightPadding: 6
                        fill: colors.overVideo
                        contentColor: colors.overVideoText
                        onClicked: cell.savePhoto()
                        AppToolTip {
                            visible: cellPhoto.hovered
                            text: cellPhoto.text + " (Ctrl+S)"
                        }
                    }

                    AppButton {
                        id: closeCell
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 8
                        visible: cellHover.hovered
                        display: AbstractButton.IconOnly
                        icon.source: "qrc:/rigelcam/icons/close.svg"
                        icon.width: 14
                        icon.height: 14
                        text: qsTr("Close")
                        implicitWidth: 26
                        implicitHeight: 26
                        leftPadding: 6
                        rightPadding: 6
                        fill: colors.overVideo
                        contentColor: colors.overVideoText
                        onClicked: playback.removeCamera(cell.index)
                        AppToolTip {
                            visible: closeCell.hovered
                            text: closeCell.text
                        }
                    }

                    HoverHandler { id: cellHover }
                }
            }

            Label {
                anchors.centerIn: parent
                visible: !win.playbackCamera || (playback.spans.length === 0 && Object.keys(playback.errors).length === 0)
                padding: 12
                textFormat: Text.PlainText
                font.pixelSize: 14
                color: colors.muted
                text: !win.playbackCamera ? qsTr("Double-click a camera to see its recordings.")
                    : playback.loading ? qsTr("Searching recordings…")
                    : qsTr("No recordings on this day.")
            }
        }

        Rectangle {
            id: controls
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 112
            color: colors.bar

            RowLayout {
                id: playerRow
                // Below this the controls get squeezed, so the speeds become a list and the gamma
                // loses its label.
                readonly property bool compact: width < 1000
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 10
                spacing: 6

                AppButton {
                    enabled: win.playbackCamera !== null && playback.viewDate !== ""
                    text: "‹"
                    implicitWidth: 34
                    onClicked: playback.stepDay(-1)
                    AppToolTip { visible: parent.hovered; text: qsTr("Previous day") }
                }
                AppButton {
                    enabled: win.playbackCamera !== null
                    text: playback.viewDate ? new Date(playback.viewDate + "T12:00:00").toLocaleDateString(Qt.locale(grid.language), Locale.ShortFormat) : ""
                    edge: colors.line
                    onClicked: calendar.open()
                    AppToolTip { visible: parent.hovered; text: qsTr("Pick a day") }

                    Popup {
                        id: calendar
                        y: -height - 6
                        modal: true
                        dim: false
                        focus: true
                        padding: 10
                        // The month on the arrows, counted from the day being played.
                        property int monthOffset: 0
                        readonly property date shown: {
                            const day = playback.viewDate ? new Date(playback.viewDate + "T12:00:00") : new Date()
                            return new Date(day.getFullYear(), day.getMonth() + monthOffset, 1)
                        }
                        // Days with recordings, by camera and month, since searching takes seconds.
                        property var knownDays: ({})
                        readonly property string monthText: win.isoDay(shown).slice(0, 7)
                        readonly property string monthKey: win.playbackCamera
                            ? win.playbackCamera.recorder + ":" + win.playbackCamera.number + ":" + monthText : ""
                        readonly property var days: knownDays[monthKey] ?? []

                        // Months whose search finished, which don't need another one.
                        property var finished: ({})

                        function search() {
                            if (!win.playbackCamera || finished[monthKey])
                                return
                            grid.searchRecordedDays(win.playbackCamera.recorder, win.playbackCamera.number, monthText)
                        }
                        // Before anyone opens it, so it opens with the days already marked.
                        onMonthKeyChanged: search()
                        onAboutToShow: monthOffset = 0

                        Connections {
                            target: grid
                            function onRecordedDaysChanged() {
                                const found = JSON.parse(grid.recordedDays)
                                const known = calendar.knownDays
                                known[found.key] = found.days
                                calendar.knownDays = Object.assign({}, known)
                                // The current month still gets new days.
                                const current = found.key.endsWith(win.isoDay(new Date()).slice(0, 7))
                                if (found.done && !current)
                                    calendar.finished[found.key] = true
                            }
                        }
                        background: Rectangle {
                            radius: 6
                            color: colors.surface
                            border.color: colors.line
                        }
                        ColumnLayout {
                            RowLayout {
                                Layout.fillWidth: true
                                AppButton {
                                    text: "‹"
                                    implicitWidth: 30
                                    onClicked: calendar.monthOffset--
                                }
                                Label {
                                    Layout.fillWidth: true
                                    horizontalAlignment: Text.AlignHCenter
                                    color: colors.text
                                    font.pixelSize: 13
                                    text: Qt.locale(grid.language).standaloneMonthName(calendar.shown.getMonth()) + " " + calendar.shown.getFullYear()
                                }
                                AppButton {
                                    text: "›"
                                    implicitWidth: 30
                                    onClicked: calendar.monthOffset++
                                }
                            }
                            MonthGrid {
                                month: calendar.shown.getMonth()
                                year: calendar.shown.getFullYear()
                                locale: Qt.locale(grid.language)
                                onClicked: date => {
                                    calendar.close()
                                    playback.changeDay(win.isoDay(date))
                                }
                                delegate: Rectangle {
                                    id: dayCell
                                    required property var model
                                    readonly property bool inMonth: model.month === calendar.shown.getMonth()
                                    readonly property bool recorded: inMonth && calendar.days.includes(model.day)
                                    implicitWidth: 34
                                    implicitHeight: 28
                                    radius: 3
                                    color: recorded ? colors.accent : "transparent"
                                    opacity: inMonth ? 1 : 0.35
                                    Text {
                                        anchors.centerIn: parent
                                        color: dayCell.recorded ? "#FFFFFF" : colors.text
                                        font.pixelSize: 13
                                        font.weight: dayCell.recorded ? Font.DemiBold : Font.Normal
                                        text: dayCell.model.day
                                    }
                                }
                            }
                        }
                    }
                }
                AppButton {
                    enabled: win.playbackCamera !== null && playback.viewDate !== "" && playback.viewDate < playback.today
                    text: "›"
                    implicitWidth: 34
                    onClicked: playback.stepDay(1)
                    AppToolTip { visible: parent.hovered; text: qsTr("Next day") }
                }

                Item { Layout.fillWidth: true }

                AppButton {
                    enabled: Playback.previousEventIn(playback.eventRuns, playback.position, playback.rate) >= 0
                    display: AbstractButton.IconOnly
                    icon.source: "qrc:/rigelcam/icons/previous-event.svg"
                    text: qsTr("Previous event")
                    contentColor: colors.waiting
                    implicitWidth: 40
                    leftPadding: 12
                    rightPadding: 12
                    onClicked: playback.previousEvent()
                    AppToolTip { visible: parent.hovered; text: parent.text }
                }
                AppButton {
                    enabled: playback.spans.length > 0
                    text: "-10"
                    implicitWidth: 40
                    leftPadding: 4
                    rightPadding: 4
                    onClicked: playback.skipBack()
                    AppToolTip { visible: parent.hovered; text: qsTr("Back 10 s") }
                }
                AppButton {
                    enabled: playback.spans.length > 0
                    text: playback.playing ? "II" : "▶"
                    implicitWidth: 40
                    leftPadding: 4
                    rightPadding: 4
                    fill: colors.raised
                    onClicked: playback.togglePlaying()
                    AppToolTip { visible: parent.hovered; text: playback.playing ? qsTr("Pause") : qsTr("Play") }
                }
                AppButton {
                    enabled: playback.spans.length > 0
                    text: "+10"
                    implicitWidth: 40
                    leftPadding: 4
                    rightPadding: 4
                    onClicked: playback.seek(playback.position + 10)
                    AppToolTip { visible: parent.hovered; text: qsTr("Forward 10 s") }
                }
                AppButton {
                    enabled: Playback.nextEventIn(playback.eventRuns, playback.position) >= 0
                    display: AbstractButton.IconOnly
                    icon.source: "qrc:/rigelcam/icons/next-event.svg"
                    text: qsTr("Next event")
                    contentColor: colors.waiting
                    implicitWidth: 40
                    leftPadding: 12
                    rightPadding: 12
                    onClicked: playback.nextEvent()
                    AppToolTip { visible: parent.hovered; text: parent.text }
                }
                Text {
                    Layout.leftMargin: 8
                    font.pixelSize: 15
                    font.weight: Font.Medium
                    font.features: { "tnum": 1 }
                    color: colors.text
                    text: playback.clockText(playback.position)
                }

                Item { Layout.fillWidth: true }

                IconButton {
                    id: exportButton
                    enabled: playback.spans.length > 0 && !grid.updateRunning
                    icon.source: "qrc:/rigelcam/icons/download.svg"
                    text: qsTr("Export clip")
                    onClicked: exportDialog.openAt(playback.position)
                }

                IconButton {
                    id: eventsButton
                    enabled: playback.events.length > 0
                    icon.source: "qrc:/rigelcam/icons/events.svg"
                    text: qsTr("Events of the day")
                    onClicked: eventsPopup.open()

                    // Stays open while it is used, so one event after another can be looked at.
                    Popup {
                        id: eventsPopup
                        x: eventsButton.width - width
                        y: -height - 6
                        width: 300
                        // From the count and not from the list, which only makes the rows that fit its
                        // height, so a height taken from it would stay at zero.
                        height: Math.min(playback.events.length * 32 + topPadding + bottomPadding, 380)
                        modal: true
                        dim: false
                        focus: true
                        padding: 4
                        background: Rectangle {
                            radius: 6
                            color: colors.surface
                            border.color: colors.line
                        }
                        // Opens on the event playing, or on the next one.
                        onOpened: {
                            const next = playback.events.findIndex(event => event.end > playback.position)
                            eventsList.positionViewAtIndex(Math.max(0, next), ListView.Center)
                        }

                        ListView {
                            id: eventsList
                            // The first event in view, which the list goes back to when a new
                            // event comes in and the model is rebuilt, instead of the top.
                            property int firstShown: 0
                            anchors.fill: parent
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: playback.events
                            onContentYChanged: {
                                const index = indexAt(0, contentY)
                                if (index >= 0)
                                    firstShown = playback.events[index].start
                            }
                            onModelChanged: {
                                if (!eventsPopup.visible)
                                    return
                                const index = playback.events.findIndex(event => event.start >= firstShown)
                                positionViewAtIndex(Math.max(0, index), ListView.Beginning)
                            }
                            ScrollIndicator.vertical: ScrollIndicator {}
                            delegate: ItemDelegate {
                                id: eventRow
                                required property var modelData
                                readonly property bool playing: playback.position >= modelData.start
                                    && playback.position < modelData.end
                                readonly property int length: modelData.end - modelData.start
                                width: ListView.view.width
                                height: 32
                                leftPadding: 8
                                rightPadding: 10
                                onClicked: playback.seek(modelData.start)
                                background: Rectangle {
                                    radius: 3
                                    color: eventRow.playing || eventRow.hovered ? colors.raised : "transparent"
                                }
                                contentItem: RowLayout {
                                    spacing: 10
                                    Rectangle {
                                        implicitWidth: 3
                                        implicitHeight: 16
                                        color: eventRow.playing ? colors.text : colors.waiting
                                    }
                                    Text {
                                        text: playback.clockText(eventRow.modelData.start)
                                        font.pixelSize: 13
                                        font.features: { "tnum": 1 }
                                        color: colors.text
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: eventRow.modelData.name
                                        // The name comes from the recorder, so it can't be read as markup.
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        font.pixelSize: 13
                                        color: colors.muted
                                    }
                                    Text {
                                        text: Math.floor(eventRow.length / 60) + ":" + String(eventRow.length % 60).padStart(2, "0")
                                        font.pixelSize: 12
                                        font.features: { "tnum": 1 }
                                        color: colors.faint
                                    }
                                }
                            }
                        }
                    }
                    tipVisible: hovered && !eventsPopup.visible
                }

                SuperResolutionButton {
                    visible: graphicsSetup.superResolution && videoArea.used.length === 1
                }
                RtxHdrButton {
                    visible: graphicsSetup.rtxHdr && videoArea.used.length === 1
                }
                GammaControl {
                    Layout.rightMargin: 8
                    compact: playerRow.compact
                }

                AppComboBox {
                    visible: playerRow.compact
                    flat: true
                    enabled: playback.spans.length > 0
                    // The text of the box keeps 40 px of padding, and below this it had no room.
                    Layout.preferredWidth: 82
                    readonly property var rates: [1, 2, 4, 8]
                    model: rates
                    labelOf: rate => rate + "x"
                    // From the plain list, since the model of a ComboBox isn't a JavaScript array.
                    currentIndex: rates.indexOf(playback.rate)
                    onActivated: index => playback.setRate(rates[index])
                }

                Repeater {
                    model: [1, 2, 4, 8]
                    AppButton {
                        required property int modelData
                        visible: !playerRow.compact
                        enabled: playback.spans.length > 0
                        text: modelData + "x"
                        implicitWidth: 40
                        leftPadding: 4
                        rightPadding: 4
                        fill: playback.rate === modelData ? colors.raised : "transparent"
                        onClicked: playback.setRate(modelData)
                    }
                }
            }

            Item {
                id: timeline
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: playerRow.bottom
                anchors.bottom: parent.bottom
                anchors.margins: 10

                // The part of the day on screen, in seconds, which the wheel changes.
                property real viewStart: 0
                property real viewEnd: 86400
                readonly property real pixelsPerSecond: width / (viewEnd - viewStart)
                // Marks sit on round times of the clock, so they slide with the bar when it moves.
                readonly property int tickStep: {
                    for (const step of [60, 120, 300, 600, 900, 1800, 3600, 7200, 10800]) {
                        if (step * pixelsPerSecond >= 70)
                            return step
                    }
                    return 21600
                }
                readonly property real firstTick: Math.ceil(viewStart / tickStep) * tickStep
                readonly property int tickCount: Math.max(0, Math.floor((viewEnd - firstTick) / tickStep) + 1)

                function xOf(seconds) {
                    return (seconds - viewStart) * pixelsPerSecond
                }

                function timeAt(x) {
                    return viewStart + x / pixelsPerSecond
                }

                function panTo(start) {
                    const span = viewEnd - viewStart
                    viewStart = win.clamp(start, 0, 86400 - span)
                    viewEnd = viewStart + span
                }

                // A zoomed bar follows the playback until the user moves it to look elsewhere,
                // and goes back to following after the next seek.
                property bool following: true

                Connections {
                    target: playback
                    function onPositionChanged() {
                        const span = timeline.viewEnd - timeline.viewStart
                        const outside = playback.position < timeline.viewStart || playback.position > timeline.viewEnd
                        if (outside && timeline.following)
                            timeline.panTo(playback.position - span * 0.1)
                    }
                }

                Rectangle {
                    id: track
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 22
                    radius: 3
                    color: colors.surface
                    clip: true

                    Repeater {
                        model: playback.spans
                        Rectangle {
                            required property var modelData
                            x: timeline.xOf(modelData.start)
                            width: Math.max(1, (modelData.end - modelData.start) * timeline.pixelsPerSecond)
                            height: track.height
                            color: modelData.event ? colors.waiting : colors.accent
                            opacity: 0.75
                        }
                    }

                    Repeater {
                        model: timeline.tickCount
                        Rectangle {
                            required property int index
                            x: timeline.xOf(timeline.firstTick + index * timeline.tickStep)
                            width: 1
                            height: track.height
                            color: colors.page
                            opacity: 0.5
                        }
                    }

                    // After the current moment nothing was recorded yet.
                    Rectangle {
                        x: timeline.xOf(playback.nowSeconds)
                        width: Math.max(0, track.width - x)
                        height: track.height
                        color: colors.page
                        opacity: 0.6
                    }

                    Rectangle {
                        x: timeline.xOf(playback.position) - 1
                        width: 2
                        height: track.height
                        color: colors.text
                    }

                    Rectangle {
                        visible: timelineMouse.containsMouse
                        x: timelineMouse.mouseX
                        width: 1
                        height: track.height
                        color: colors.text
                        opacity: 0.5
                    }
                }

                Repeater {
                    model: timeline.tickCount
                    Text {
                        required property int index
                        readonly property real time: timeline.firstTick + index * timeline.tickStep
                        y: track.height + 4
                        x: timeline.xOf(time) - implicitWidth / 2
                        visible: x >= 0 && x + implicitWidth <= timeline.width
                        font.pixelSize: 11
                        color: colors.muted
                        text: playback.clockText(time).slice(0, 5)
                    }
                }

                MouseArea {
                    id: timelineMouse
                    property real pressX: 0
                    property real pressStart: 0
                    property bool panning: false
                    anchors.fill: track
                    hoverEnabled: true
                    cursorShape: panning ? Qt.ClosedHandCursor : Qt.PointingHandCursor
                    onPressed: mouse => {
                        pressX = mouse.x
                        pressStart = timeline.viewStart
                        panning = false
                    }
                    onPositionChanged: mouse => {
                        // With hover on this also runs without a button held, which isn't a drag.
                        if (!pressed)
                            return
                        if (Math.abs(mouse.x - pressX) > 4)
                            panning = true
                        if (panning) {
                            timeline.following = false
                            timeline.panTo(pressStart - (mouse.x - pressX) / timeline.pixelsPerSecond)
                        }
                    }
                    // Dragging only moves the bar, so it never changes what is playing by accident.
                    onReleased: mouse => {
                        if (!panning)
                            playback.seek(timeline.timeAt(mouse.x))
                        panning = false
                    }
                    onWheel: wheel => {
                        const shift = wheel.modifiers & Qt.ShiftModifier
                        if (shift || wheel.angleDelta.x !== 0) {
                            // With Shift, some systems report the wheel as horizontal and inverted.
                            const delta = shift ? (wheel.angleDelta.y || -wheel.angleDelta.x) : wheel.angleDelta.x
                            const span = timeline.viewEnd - timeline.viewStart
                            timeline.following = false
                            timeline.panTo(timeline.viewStart - delta / 120 * span * 0.1)
                            return
                        }
                        const anchor = timeline.timeAt(wheel.x)
                        const span = timeline.viewEnd - timeline.viewStart
                        const next = win.clamp(span * Math.pow(0.9985, wheel.angleDelta.y), 300, 86400)
                        const start = win.clamp(anchor - (anchor - timeline.viewStart) * next / span, 0, 86400 - next)
                        timeline.viewStart = start
                        timeline.viewEnd = start + next
                        // Zoomed in on another time, the bar stays there.
                        if (playback.position < start || playback.position > start + next)
                            timeline.following = false
                    }
                }

                // The time under the mouse, to aim at a moment before clicking it. It covers the
                // marks of the hours below the bar while it shows.
                Rectangle {
                    visible: timelineMouse.containsMouse
                    x: win.clamp(timelineMouse.mouseX - width / 2, 0, timeline.width - width)
                    y: track.height + 2
                    width: hoverTime.implicitWidth + 10
                    height: hoverTime.implicitHeight + 4
                    radius: 3
                    color: colors.raised
                    Text {
                        id: hoverTime
                        anchors.centerIn: parent
                        font.pixelSize: 11
                        font.features: { "tnum": 1 }
                        color: colors.text
                        text: playback.clockText(win.clamp(timeline.timeAt(timelineMouse.mouseX), 0, 86399))
                    }
                }
            }
        }
    }

    Timer {
        id: fullScreenHint
        interval: 3000
    }

    Rectangle {
        visible: fullScreenHint.running && win.fullScreen
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        y: 14
        width: hintText.implicitWidth + 24
        height: 32
        radius: 4
        color: colors.panel
        border.color: colors.line
        Text {
            id: hintText
            anchors.centerIn: parent
            font.pixelSize: 13
            color: colors.text
            text: qsTr("F11 or Esc to leave full screen")
        }
    }

    Rectangle {
        id: dragLabel
        visible: win.dragged !== null
        z: 10
        width: dragName.implicitWidth + 24
        height: 32
        radius: 4
        color: colors.raised
        border.color: colors.accent
        Drag.active: visible
        Drag.hotSpot.x: width / 2
        Drag.hotSpot.y: height / 2
        Text {
            id: dragName
            anchors.centerIn: parent
            text: win.dragged ? win.dragged.name : ""
            textFormat: Text.PlainText
            font.pixelSize: 13
            font.weight: Font.Medium
            color: colors.text
        }
    }

    // Here and not with the other notices, since the top bar is hidden while zoomed.
    Rectangle {
        id: photoToast
        property string title: ""
        property string folder: ""
        property string fileName: ""
        property bool failed: false

        function show(file) {
            const path = file.toString()
            const slash = path.lastIndexOf("/")
            failed = path === ""
            title = failed ? qsTr("Could not save the photo.") : qsTr("Photo saved")
            folder = failed ? "" : path.slice(0, slash)
            fileName = failed ? "" : decodeURIComponent(path.slice(slash + 1))
            hideToast.restart()
        }

        function showVideo(savedIn, file, error) {
            failed = error !== ""
            title = failed ? win.msg(error) : qsTr("Video saved")
            const prefix = Qt.platform.os === "windows" ? "file:///" : "file://"
            // encodeURI keeps # and ?, which a URL would read as the end of the path.
            folder = failed ? "" : encodeURI(prefix + savedIn.replace(/\\/g, "/")).replace(/#/g, "%23").replace(/\?/g, "%3F")
            fileName = file
            hideToast.restart()
        }

        visible: hideToast.running
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: win.inRecordings ? 124 : 20
        width: toastRow.implicitWidth + 28
        height: 54
        radius: 6
        color: colors.panel
        border.color: colors.line

        Timer {
            id: hideToast
            interval: 6000
        }
        RowLayout {
            id: toastRow
            anchors.centerIn: parent
            spacing: 14
            Column {
                Text {
                    text: photoToast.title
                    textFormat: Text.PlainText
                    font.pixelSize: 13
                    font.weight: Font.Medium
                    color: photoToast.failed ? colors.danger : colors.text
                }
                Text {
                    visible: photoToast.folder !== ""
                    text: photoToast.fileName
                    // The name comes from the recorder, so it can't be read as markup.
                    textFormat: Text.PlainText
                    font.pixelSize: 12
                    color: colors.muted
                }
            }
            AppButton {
                visible: photoToast.folder !== ""
                text: qsTr("Open folder")
                edge: colors.line
                onClicked: Qt.openUrlExternally(photoToast.folder)
            }
        }
    }

    Rectangle {
        // Closed for this session with "Not now". The next start offers it again.
        property bool dismissed: false
        visible: grid.updateVersion !== "" && !dismissed
        z: 10
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 20
        // Above the controls of the recordings, like the other notices.
        anchors.bottomMargin: win.inRecordings ? 124 : 20
        width: Math.min(420, parent.width - 32)
        height: updateLayout.implicitHeight + 24
        radius: 6
        // Solid, so the grid under it doesn't get in the way of reading.
        color: colors.surface
        border.color: colors.line
        ColumnLayout {
            id: updateLayout
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8
            Text {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                font.pixelSize: 13
                color: grid.updateError !== "" ? colors.danger : colors.text
                text: grid.updateError !== "" ? win.msg(grid.updateError)
                    : win.updateAfterExport ? qsTr("The update starts when the export ends.")
                    : grid.updateRunning ? qsTr("Downloading version %1").arg(grid.updateVersion)
                        + "  " + Math.round(grid.updateProgress * 100) + "%"
                    : qsTr("Version %1 is available.").arg(grid.updateVersion)
            }
            Rectangle {
                visible: grid.updateRunning
                Layout.fillWidth: true
                implicitHeight: 4
                radius: 2
                color: colors.line
                Rectangle {
                    width: parent.width * grid.updateProgress
                    height: parent.height
                    radius: 2
                    color: colors.accent
                }
            }
            RowLayout {
                visible: !grid.updateRunning && !win.updateAfterExport
                Layout.alignment: Qt.AlignRight
                spacing: 8
                AppButton {
                    text: qsTr("Not now")
                    edge: colors.line
                    onClicked: parent.parent.parent.dismissed = true
                }
                PrimaryButton {
                    text: qsTr("Update")
                    onClicked: {
                        if (grid.exportRunning)
                            win.updateAfterExport = true
                        else
                            grid.installUpdate()
                    }
                }
            }
        }
    }

    Rectangle {
        visible: grid.exportRunning
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: (win.inRecordings ? 124 : 20) + (photoToast.visible ? 62 : 0)
        width: Math.min(420, parent.width - 32)
        height: 62
        radius: 6
        color: colors.panel
        border.color: colors.line
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 10
            spacing: 12
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                Text {
                    Layout.fillWidth: true
                    // The name comes from the recorder, so it can't be read as markup.
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    font.pixelSize: 13
                    color: colors.text
                    text: (grid.exportsWaiting > 0 ? qsTr("Exporting %1 (%2 more)").arg(grid.exporting).arg(grid.exportsWaiting)
                        : qsTr("Exporting %1").arg(grid.exporting))
                        + "  " + Math.round(grid.exportProgress * 100) + "%"
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 4
                    radius: 2
                    color: colors.line
                    Rectangle {
                        width: parent.width * grid.exportProgress
                        height: parent.height
                        radius: 2
                        color: colors.accent
                    }
                }
            }
            AppButton {
                text: qsTr("Cancel")
                edge: colors.line
                onClicked: grid.cancelExports()
            }
        }
    }
}

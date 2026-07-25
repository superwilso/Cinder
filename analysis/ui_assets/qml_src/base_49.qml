//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
    id: base
    property alias title: title.text
    width: 480
    height: 90
    color: "black"

    property bool is_source_direct: false
    property bool is_clear_audio_plus: false

    QtObject {
        id: p
        readonly property int textLeftMargin: 20
        readonly property int textRightMargin: 20
        readonly property int titleRightMargin: 88
        readonly property real titleDispVisible: 1.0
        readonly property real titleDispInvisible: 0.0
        readonly property int fadeInMillisecond: 100
    }

    SCmnLabel {
        id: title
        anchors { top: parent.top; bottom: parent.bottom; left: parent.left; leftMargin: p.textLeftMargin; right: parent.left; rightMargin: p.titleRightMargin; }
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignLeft
        color: viewstyle.textcolor.L1
        font {pixelSize: viewstyle.textsize.L; weight: Font.Light; }
    }

    Image {
        id: main_separator
        anchors { bottom: base.bottom; left: base.left; }
        source: "qrc:/assets/images/separator_main.png"
    }

    /* フェード処理 */
    states: [
        State {
            when: is_source_direct || is_clear_audio_plus
            PropertyChanges { target: title; opacity: p.titleDispInvisible }
        },
        State {
            when: !is_source_direct && !is_clear_audio_plus
            PropertyChanges { target: title; opacity: p.titleDispVisible }
        }
    ]
    transitions: Transition {
        NumberAnimation { property: "opacity"; duration: p.fadeInMillisecond }
    }
}

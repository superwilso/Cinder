//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/framework"

Item {
    id: base
    width: 480
    height: 542

    property bool dialog_open
    property bool dialog_animation

    QtObject {
        id: p
        readonly property double dialog_disp_visible: 1.0
        readonly property double dialog_disp_invisible: 0.0
        readonly property int dialog_disp_animation_duration: 100
        readonly property int dialog_disp_no_animation_duration: 0
    }

    UsbAudioConnectingBase {
        id: usbAudioDialog
        anchors.fill: parent
    }
    states: [
        State {
            when: base.dialog_open
            PropertyChanges { target: usbAudioDialog; opacity: p.dialog_disp_visible }
        },
        State {
            when: !base.dialog_open
            PropertyChanges { target: usbAudioDialog; opacity: p.dialog_disp_invisible }
        }
    ]
    transitions: [
        Transition {
            NumberAnimation { property: "opacity"; duration: base.dialog_animation ? p.dialog_disp_animation_duration : dialog_disp_no_animation_duration; }
        }
    ]
}

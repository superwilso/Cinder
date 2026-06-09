//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/framework"
import "qrc:/parts"

Item {
    id: base
    width: 480
    height: 408

    property int device_type
    property bool dialog_animation

    QtObject {
        id: p
        readonly property double dialog_disp_visible: 0.7
        readonly property double dialog_disp_invisible: 0.0
        readonly property double dialog_message_visible: 1.0
        readonly property double dialog_message_invisible: 0.0
        readonly property int dialog_disp_animation_duration: 100
        readonly property int dialog_disp_no_animation_duration: 0

        readonly property int text_horizontal_margin: 20
        readonly property int dialog_message_line_spacing: 36
    }
    onDevice_typeChanged: {
        dialogMessageSetting()
    }

    function dialogMessageSetting(){
        if(3 == base.device_type){
            dialogMessage.text = qsTr("020270") + viewstyle.empty_string
        }
        else if(4 == base.device_type){
            dialogMessage.text = qsTr("020271") + viewstyle.empty_string
        }
        else {}
    }

    Connections {
      target: controller
      onAvailable: {
          dialogMessageSetting()
      }
    }

    Rectangle {
        id: connectingDialog
        anchors.fill: parent
        color: viewstyle.bgcolor.D1
    }
    SCmnLabel{
        id: dialogMessage
        anchors { verticalCenter: connectingDialog.verticalCenter; left: connectingDialog.left; right: connectingDialog.right;
                  leftMargin: p.text_horizontal_margin; rightMargin: p.text_horizontal_margin;
        }
        font.pixelSize: viewstyle.textsize.S
        font.weight: Font.Normal
        color: viewstyle.textcolor.L2
        lineHeightMode: Text.FixedHeight
        lineHeight: p.dialog_message_line_spacing
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    states: [
        State {
            when: ((3 == base.device_type) || (4 == base.device_type))
            PropertyChanges { target: connectingDialog; opacity: p.dialog_disp_visible }
            PropertyChanges { target: dialogMessage;    opacity: p.dialog_message_visible }
        },
        State {
            when: !((3 == base.device_type) || (4 == base.device_type))
            PropertyChanges { target: connectingDialog; opacity: p.dialog_disp_invisible }
            PropertyChanges { target: dialogMessage;    opacity: p.dialog_message_invisible }
        }
    ]
    transitions: [
        Transition {
            NumberAnimation { property: "opacity"; duration: base.dialog_animation ? p.dialog_disp_animation_duration : p.dialog_disp_no_animation_duration; }
        }
    ]
}

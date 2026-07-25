//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/framework"
import "qrc:/parts"

Item{
    id: base
    visible: true
    width: parent.width
    height: parent.height

    QtObject {
        id: p
        readonly property int text_left_margin: 20
        readonly property int text_right_margin: 20
        readonly property int dialog_heading_top_margin: 190
        readonly property int dialog_message_height: (p.dialog_message_line_spacing * p.text_maximum_line_count)
        readonly property int dialog_message_top_margin: 72
        readonly property int dialog_message_line_spacing: 36
        readonly property int text_maximum_line_count: 5
    }

    Rectangle {
        id: dialogDisp
        anchors.fill: parent
        color: viewstyle.bgcolor.D1
    }

    SCmnLabel{
        id: dialogHeading
        anchors { top: base.top;     topMargin: p.dialog_heading_top_margin;
                  left: base.left;   leftMargin: p.text_left_margin;
                  right: base.right; rightMargin: p.text_right_margin }
        text: qsTr("200042") + viewstyle.empty_string
        font.pixelSize: viewstyle.textsize.LL
        font.weight: Font.Light
        color: viewstyle.textcolor.L1
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    SCmnLabel{
        id: dialogMessage
        height: p.dialog_message_height
        anchors { top: dialogHeading.bottom; topMargin: p.dialog_message_top_margin;
                  left: base.left;           leftMargin: p.text_left_margin;
                  right: base.right;         rightMargin: p.text_right_margin }
        text: qsTr("020223") + viewstyle.empty_string
        font.pixelSize: viewstyle.textsize.S
        font.weight: Font.Normal
        color: viewstyle.textcolor.L2
        lineHeightMode: Text.FixedHeight
        lineHeight: p.dialog_message_line_spacing
        maximumLineCount: p.text_maximum_line_count
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignLeft
        verticalAlignment: Text.AlignTop
    }
}

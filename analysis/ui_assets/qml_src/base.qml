//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0
import "qrc:/components"
import "qrc:/parts"

Item {
    id: base

    property string playTargetName: "My Favorite Hi-Res Songs"
    property string playIndexAndTotalCount: "10/10"

    width: 480
    height: 36

    QtObject {
        id: p
        property real fontSize: 16
    }

    Rectangle {
        id: background
        anchors.fill: parent
        color: "black"
    }

    SCmnLabel {
        id: label1
        width: 342
        height: base.height
        anchors{top: parent.top; left: parent.left; leftMargin: 8}
        text: base.playTargetName
        font.pointSize: p.fontSize
        verticalAlignment: Text.AlignVCenter
    }
    SCmnLabel {
        id: label2
        width: 82
        height: base.height
        anchors{top: parent.top; left: label1.right}
        text: base.playIndexAndTotalCount
        font.pointSize: p.fontSize
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    SettingIconArea {
        id: setting_icon
        width: base.width - label1.width - label2.width
        height: base.height
        anchors{top: parent.top; left: label2.right}
    }
}


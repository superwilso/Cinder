//
// Copyright 2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item {
    QtObject {
        id: p
        readonly property int textAreaWidth: 321
        readonly property int textAreaHeight: 99
        readonly property int lineSpacing: 33
        readonly property int maximumLineCount: 3
    }
    // モーダルを実現
    Rectangle {
        id: grayout
        width: parent.width
        height: parent.height
        color: "black"
        opacity: 0.8

        MouseArea {
            anchors.fill: parent
            preventStealing: true
            enabled: visible
        }
    }
    // 外音マイコンエラー警告ダイアログ
    Rectangle {
        id: dialog
        anchors.centerIn: parent
        width: 361
        height: messageArea.height + 2 // メッセージ + 上下margine
        color: "#00000000"      // 透過指定

        BorderImage {
          id: centerImage
          source: "qrc:/assets/images/popup_area_center_4r.png" // 361 x 90
          width: dialog.width; height: dialog.height
          border.left: 3; border.top: 3
          border.right: 3; border.bottom: 3
          horizontalTileMode: BorderImage.Repeat
        }

        SCmnCenterText {
            id: messageArea
            text: qsTr("010130") + viewstyle.empty_string
            width: parent.width
            height: childrenRect.height
            anchors.top: parent.top
            anchors.topMargin: 1
        }
    }
}

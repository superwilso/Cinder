//
// Copyright 2015-2017 Sony Corporation
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
    // 空電池中央配置用の空のアイテム
    Item {
      id: dummy
      anchors.centerIn: parent
      width: 361
      height: 88 * 4 +2 // 画像高さを考慮して+2
    }
    // 減電警告ダイアログ
    Rectangle {
        id: dialog
        anchors {
            top: dummy.top; left: dummy.left; right: dummy.right;
        }
        width: 361
        height: 88 * 3 + 2 // 画像高さを考慮して+2
        color: "#00000000"      // 透過指定

        BorderImage {
          id: centerImage
          source: "qrc:/assets/images/popup_area_center_4r.png" // 361 x 90
          width: dialog.width; height: dialog.height
          border.left: 3; border.top: 3
          border.right: 3; border.bottom: 3
          horizontalTileMode: BorderImage.Repeat
        }
    }
    // 空電池
    Item {
        id: lowBattery
        width: dialog.width
        height: 88 * 2 + 1 // 背景画像高さを考慮して+1
        anchors {
            bottom: dialog.bottom; left: dialog.left; right: dialog.right;
        }
        Image {
            anchors {
                horizontalCenter: parent.horizontalCenter
                verticalCenter: parent.verticalCenter
            }
            source: "qrc:/assets/images/ic_lowbattery_alert_dialog_empty.png"
        }
    }
    // メッセージ
    SCmnLabel {
        id: message
        width: p.textAreaWidth
        height: p.textAreaHeight
        maximumLineCount: p.maximumLineCount
        lineHeightMode: Text.FixedHeight
        lineHeight: p.lineSpacing
        anchors {
            top: dialog.top; left: dialog.left; right: dialog.right;
            leftMargin: 20; rightMargin: 20; topMargin: 12;
        }
        text: qsTr("010002") + viewstyle.empty_string
        verticalAlignment: Text.AlignVCenter
        font.pixelSize: viewstyle.textsize.S
        font.weight: Font.Light
        color: viewstyle.textcolor.L1
    }
}

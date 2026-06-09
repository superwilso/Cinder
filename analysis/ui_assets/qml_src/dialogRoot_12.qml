//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0
import QtQuick.Controls 1.0

Item {
    id: dialogRoot

    // シグナル
    signal closed()

    // タイムアウトタイマ
    Timer {
        id: timeOutTimer
        interval: dmpDialog.timeout_interval // 0の時はタイマーが走らない
        running: true
        onTriggered: dialogRoot.closed()
    }   

    // タイムアウトダイアログ
    Rectangle {
        id: dialog
        anchors.centerIn: parent
        width: 200
        height:  topText.height + bottomText.paintedHeight + 35
        border.width: 1

        // ダイアログタッチ
        MouseArea {
            anchors.fill: parent
            onClicked: {
                timeOutTimer.running = false;
                dialogRoot.closed()
            }
        }

        // 上部テキスト
        Text {
            id: topText
            anchors.top: parent.top
            anchors.topMargin: 10
            anchors.left: parent.left
            anchors.leftMargin: 10
            anchors.right: parent.right
            anchors.rightMargin: 10
            wrapMode: Text.Wrap
            text: qsTr(dmpDialog.top_message)
            height: 40
            verticalAlignment: Text.AlignVCenter
            maximumLineCount: 2
            elide: Text.ElideRight
        }

        // 下部テキスト
        Text {
            id: bottomText
            anchors.top: topText.bottom
            anchors.topMargin: 10
            anchors.left: parent.left
            anchors.leftMargin: 10
            anchors.right: parent.right
            anchors.rightMargin: 10
            wrapMode: Text.Wrap
            text: qsTr(dmpDialog.bottom_message)
        }
    }
}

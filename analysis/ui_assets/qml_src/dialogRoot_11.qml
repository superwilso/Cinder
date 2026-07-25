//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.0
import QtQuick.Controls 1.0

Item {
    id: dialogRoot

    // シグナル
    signal progressCompleted()

    // 処理中ダイアログ（Cancel不可）
    Rectangle {
        id: dialog
        anchors.centerIn: parent
        width: 200
        height: topText.height + bottomText.paintedHeight + progressBar.height + 40
        border.width: 1

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

        // プログレスバー
        ProgressBar {
            id: progressBar
            anchors.top: bottomText.bottom
            anchors.topMargin: 10
            anchors.left: parent.left
            anchors.leftMargin: 10
            anchors.right: parent.right
            anchors.rightMargin: 10
            maximumValue: 10
            value: dmpDialog.progress
            indeterminate: true
            onValueChanged: {
                if (value == 10)
                {
                    waitTimer.running = true
                }
            }
            // waitタイマー
            Timer {
                id: waitTimer
                interval: 100
                running: false
                onTriggered: dialogRoot.progressCompleted()
            }
        }
    }
}

//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Dialogs 1.2

Item {
    id: dialogRoot

    // シグナル
    signal confirmed()

    // Okダイアログ
    Rectangle {
        id: dialog
        anchors.centerIn: parent
        width: 200
        height:  topText.height + bottomText.paintedHeight + buttonRow.height + 35
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

        // ボタン行
        Row {
            id: buttonRow
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 5
            Button {
                text: qsTr("Ok")
                onClicked: dialogRoot.confirmed()
            }
        }
    }
}

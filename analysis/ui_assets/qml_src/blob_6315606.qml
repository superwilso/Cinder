//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

// シャットダウン画面
Rectangle {
    width: parent.width
    height: parent.height
    anchors.centerIn: parent
    color: "black"

    // シャットダウンロゴ
    Image {
        anchors.centerIn: parent
        fillMode: Image.Pad
        source: "qrc:/sys_shutdown_logo.png"
    }
    // マウスイベント盗難防止
    MouseArea {
        anchors.fill: parent
        preventStealing: true
    }
}

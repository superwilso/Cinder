//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Item {
    id: base

    readonly property int barWidth: 480
    readonly property int barHeight: 8
    readonly property color baseColor: viewstyle.textcolor.L3   // Volume Bar OFF表示色
    readonly property color barColor: viewstyle.textcolor.L1    // Volume Bar ON表示色

    property int maxValue: 120
    property int currentValue: 0
    property int oneValue: barWidth / maxValue
    property bool visibleAvlsMark: false
    property int avlsVolumeMax: 0                       // AVLS Volume Max値
    property bool isAvlsOn: false

    width: barWidth
    height: barHeight

    // 基底(グレー色)
    Rectangle {
        id: barBase
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.leftMargin: -1      // volume bar 左端のみマスク画像の1Blockが 2dotのため.
        color: baseColor
        height: barHeight
        width: isAvlsOn === false ? parent.width : (oneValue * avlsVolumeMax)
        clip: true

        // ボリューム
        Rectangle {
            id: barVolume
            anchors.top: barBase.top
            anchors.left: barBase.left
            color: barColor
            height: barBase.height
            width: currentValue * oneValue
        }
    }
    // ボリューム色用のMASK画像を重ねる
    Image {
        id: barFillter
        anchors.top: barBase.top
        anchors.left: barBase.left
        source: "qrc:/assets/images/ic_volumebar_volume_meter_template.png"
        clip: true
        fillMode: Image.Pad
        width: isAvlsOn === false ? parent.width : (oneValue * avlsVolumeMax)
        height: parent.height
    }

    // AVLSマーク
    Image {
        id: avlsMark
        visible: visibleAvlsMark
        anchors.top: base.top
        anchors.right: base.right
        source: "qrc:/assets/images/ic_volumebar_setting_avls.png"
    }
}


//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/components"

Item {
    id: root

    property real valueLeft
    property real valueRight

    width: meterBackground.width
    height: meterBackground.height

    Image {
        id: meterBackground
        anchors{left: parent.left; top: parent.top}
        source: "qrc:/ic_audio_player_vu_meter_background.png"
    }
    LevelMeterNeedle {
        id: levelMeterLeft
        anchors{left: parent.left; top: anchors.top}
        value: root.valueLeft
    }
    LevelMeterNeedle {
        id: levelMeterRight
        anchors{left: levelMeterLeft.right; top: anchors.top}
        value: root.valueRight
    }
    Image {
        id: meterFront
        anchors{left: parent.left; top: parent.top}
        source: "qrc:/ic_audio_player_vu_meter_front.png"
    }
}

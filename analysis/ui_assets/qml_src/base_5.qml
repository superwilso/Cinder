//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/components"

Item {
    id: base

    property var spectrumIndex
    property var spectrumColor
    property var meterOpacity
    property var peakIndex

    width: meterImage.width
    height: meterImage.height
    clip: true

    QtObject {
        id: p
        property int numOfMeter: 31
        property int meterHeight: 10
    }

    //Spectrum Meter
    Rectangle {
        id: spectrumMeter
        anchors.left: parent.left
        border.width: 0
        width: base.width
        height: base.height
        color: base.spectrumColor
        opacity: base.meterOpacity
        y: ((0 <= base.spectrumIndex) && (base.spectrumIndex < p.numOfMeter))
           ? (base.spectrumIndex * p.meterHeight)
           : (p.numOfMeter * p.meterHeight)
    }

    //Peak Hold
    Rectangle {
        id: peakHoldMeter
        anchors.left: parent.left
        border.width: 0
        width: parent.width
        height: p.meterHeight
        color: base.spectrumColor
        opacity: base.meterOpacity
        y: ((0 <= base.peakIndex) && (base.peakIndex < p.numOfMeter))
           ? (base.peakIndex * p.meterHeight)
           : (p.numOfMeter * p.meterHeight)
    }

    //Spectrum Meter Image
    Image {
        id: meterImage
        anchors{left: parent.left; top: parent.top}
        source: "qrc:/ic_audio_player_spectrum_analyzer_meter_unit_template.png"
    }
}

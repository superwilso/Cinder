//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/components"

Item {
    id: base

    //SpectrumAnalyzerコンポーネントに不定値が伝わらないように、spectrum1-12に初期値を入れておく。
    //初期値は31以上の値ならよい。
    property var spectrumIndexList: [100, 100, 100, 100,
                                     100, 100, 100, 100,
                                     100, 100, 100, 100]
    //SpectrumAnalyzerコンポーネントに不定値が伝わらないように、peakhold1-12に初期値を入れておく。
    //初期値は31以上の値ならよい。
    property var peakholdIndexList: [100, 100, 100, 100,
                                     100, 100, 100, 100,
                                     100, 100, 100, 100]

    width: labelImage.width
    height: spectrumBar.height + labelImage.height

    QtObject {
        id: p
        property int numberOfSpectrumBar: 12
        property var spectrumColor: ["#ffffff", "#ffffff", "#ffffff", "#ffffff",
                                     "#ffffff", "#ffffff", "#ffffff", "#ffffff",
                                    "#ffffff", "#ffffff", "#ffffff", "#c0a565"]
        property var meterOpacity: [0.6, 0.6, 0.6, 0.6,
                                    0.6, 0.6, 0.6, 0.6,
                                    0.6, 0.6, 0.6, 1.0]
    }

    function frameUpdate(arg_spectrum, arg_peakhold)
    {
        base.spectrumIndexList = arg_spectrum
        base.peakholdIndexList = arg_peakhold
    }

    Row {
        id: spectrumRow
        anchors{top: parent.top; horizontalCenter: parent.horizontalCenter}
        Repeater {
            id: spectrumBar
            model: p.numberOfSpectrumBar
            SpectrumBar {
                spectrumIndex: base.spectrumIndexList[index]
                spectrumColor: p.spectrumColor[index]
                meterOpacity: p.meterOpacity[index]
                peakIndex: base.peakholdIndexList[index]
            }
        }
    }
    Image {
        id: labelImage
        anchors{top: spectrumRow.bottom; left: parent.left}
        source: "qrc:/ic_audio_player_spectrum_analyzer_band_values.png"
    }
}

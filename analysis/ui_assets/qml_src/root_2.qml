//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3

Item {
    id: root
    width: 240
    height: p.needleImageHeight * 2 + 20

    property real value

    QtObject {
        id: p
        property real needleImageHeight: 260
        // angleFactorの値100000000.0を変更する場合は、
        // MusicPlayerLevelMeterViewModel.cpp ConvertLevelToAngle(level)メソッド内変数
        // angle_coeffの係数100000000.0を同じ値に変更する必要がある。
        property real angleFactor: 100000000.0
    }
    onValueChanged: {
        canvas.requestPaint()
    }
    Canvas {
        id: canvas
        width: root.width
        height: p.needleImageHeight * 2
        anchors{left: parent.left; bottom: parent.bottom}
        antialiasing: true
        renderTarget: Canvas.Image
        renderStrategy: Canvas.Immediate
        property string needleImageUrl: "qrc:/ic_audio_player_vu_meter_pointer.png"
        Component.onCompleted: {
            loadImage(canvas.needleImageUrl)
        }
        onImageLoaded: {
            requestPaint()
        }
        onPaint: {
            var ctx = getContext("2d")
            ctx.clearRect(0, 0, canvas.width, canvas.height)
            ctx.translate(canvas.width/2, canvas.height/2)
            // root.valueには針の振れ角（単位rad）を10^8倍した値が入力されるため、
            // root.valueを10^-8倍して回転角を算出する必要がある。
            ctx.rotate(root.value / p.angleFactor)
            ctx.drawImage(canvas.needleImageUrl, 0, -canvas.height/2)
            ctx.resetTransform()
        }
    }
}

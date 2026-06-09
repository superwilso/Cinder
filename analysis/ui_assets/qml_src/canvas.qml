//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief プルダウン用ボタン
 * 画像への差し替えにより不要となる可能性あり
　*/
import QtQuick 2.3

Item {
    /** type color ラインカラー */
    property color lineColor: "white"
    /** type int ライン幅 */
    property int lineWidth: 1
    onLineWidthChanged:requestPaint();

    height: lineWidth
    Canvas {
        id: canvas
        anchors.fill: parent
        antialiasing: true

        property bool fill: true
        property bool stroke: true
        property real rotate : 0
        onFillChanged:requestPaint();
        onStrokeChanged:requestPaint();
        onRotateChanged:requestPaint();
        onPaint: {
            var ctx = canvas.getContext('2d')
            var w = canvas.width, h = canvas.height
            ctx.save()
            ctx.clearRect(0, 0, w, h)
            ctx.strokeStyle = lineColor
            ctx.fillStyle = lineColor
            ctx.lineWidth = lineWidth
            ctx.translate(0, 0)
            ctx.beginPath()
            ctx.lineTo(0, h / 3)
            ctx.lineTo(w / 2, h * 2 / 3)
            ctx.lineTo(w, h / 3)
            ctx.lineTo(w - 2, h / 3)
            ctx.lineTo(w / 2, h * 2 / 3 - 2)
            ctx.lineTo(2, h / 3)
            ctx.closePath()
            if (canvas.fill) ctx.fill()
            if (canvas.stroke) ctx.stroke()
            ctx.restore()
        }
    }
}


//
// Copyright 2017 Sony Corporation
//
import QtQuick 2.3

Item {
  id: base

  /** type:var ピークレベルの値 */
  property var level_index
  /** type:var ピークホールド対象のインデックス値 */
  property var peak_hold_index

  width: peakMeterImage.sourceSize.width
  height: peakMeterImage.sourceSize.height
  clip: true

  QtObject {
    id: p
    property int num_of_meter: 31
    property int meter_width: 13  // 11+2(マージン分領域)
    property string meter_color: "#ffffff"
    property real meter_opacity: 1.0
    property real back_opacity: 0.25
    property string meter_image: "qrc:/ic_dac_level_meter_scale.png"
  }

  //Background
  Rectangle {
    id: background
    anchors.fill: parent
    color: p.meter_color
    opacity: p.back_opacity
  }

  //Peak Meter
  Rectangle {
    id: peakMeter
    width: peakMeterImage.sourceSize.width
    height: peakMeterImage.sourceSize.height
    anchors.top: parent.top
    color: p.meter_color
    opacity: p.meter_opacity
    // メモリは、左がマイナス、右がプラスに対して、level_indexは、31～0となる。
    // そのため、index値と1メモリ分の幅に対して、-1をかけています。
    x: ((0 <= base.level_index) && (base.level_index < p.num_of_meter))
       ? (base.level_index * p.meter_width * (-1))
       : -peakMeter.width
  }

  //Peak Hold
  Rectangle {
    id: peakHold
    width: p.meter_width    // PeakHoldは、1マス分のサイズ設定
    height: peakMeterImage.sourceSize.height
    anchors.top: parent.top
    color: p.meter_color
    opacity: p.meter_opacity
    // X座標の位置を指定することで、ピークメーターのメーター位置を決定しています。
    // メーターが1の時は、X座標=0、メーターが0の時は、欄外となるように計算しています。
    x: ((0 <= base.peak_hold_index) && (base.peak_hold_index < p.num_of_meter))
       ? ((p.num_of_meter - 1 - base.peak_hold_index) * p.meter_width)
       : -peakHold.width
  }

  // meter image
  Image {
    id: peakMeterImage
    anchors { top: parent.top; left: parent.left; }
    source: p.meter_image
  }
}

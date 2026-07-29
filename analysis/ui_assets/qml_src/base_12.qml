//
// Copyright 2017 Sony Corporation
//
import QtQuick 2.3
import "qrc:/components"

Item {
  id: base

  QtObject {
    id: p
    property int number_of_meter: 2
    property int total_height: 52
    property int label_left_margin: 15
    property int meter_left_margin: 6
    property int meter_spacing: 4  // 16-6-6=4 ※meter画像のmeter部分の間隔16pxから上下のmeter以外部分のpxを差し引いた分が画像間のマージン
    property string meter_value_image: "qrc:/ic_dac_level_meter_label_level_value.png"
    property string meter_label_image: "qrc:/ic_dac_level_meter_label_lr.png"
    readonly property int invalid_index: 32 // メーターの最大値+1を設定

    //PeakMeterコンポーネントに不定値が伝わらないように、初期値を入れておく。
    //初期値は31以上の値ならよい。
    //31以上の値に設定することで、メーター表現が0の状態となる。
    property var level_index_list: [invalid_index, invalid_index]
    property var peak_hold_index_list: [invalid_index, invalid_index]
  }
  height: p.total_height

  /**
   * レベル値、ピークホールド値更新関数
   * @param arg_level    レベル値
   * @param arg_peakhold ピークホールド値
   */
  function frameUpdate(arg_level, arg_peakhold)
  {
    p.level_index_list = arg_level
    p.peak_hold_index_list = arg_peakhold
  }

  Image {
    id: meterValueImage
    anchors { top: parent.top; left: parent.left; }
    source: p.meter_value_image // 480*16
  }
  Image {
    id: meterLabelImage
    anchors { top: meterValueImage.bottom; left: parent.left; leftMargin: p.label_left_margin; }
    source: p.meter_label_image // 18*36
  }
  Column {
    id: peakMeter
    anchors{ top: meterValueImage.bottom; left: meterLabelImage.right; leftMargin: p.meter_left_margin; }
    spacing: p.meter_spacing
    Repeater {
      model: p.number_of_meter
      PeakMeterBar {
        level_index: p.level_index_list[index]
        peak_hold_index: p.peak_hold_index_list[index]
      }
    }
  }
}

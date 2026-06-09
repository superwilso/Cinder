//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"

Item
{
  /** paramにはQVariantListがバインドされる。データの位置、種類はStatusBarListModelのenum BatteryRemainingImageDataPosに従う */
  property var param
  /** アニメーション用の画像ファイル名がバインドされる */
  property var animationImage
  /** アニメーション用の画像ファイルパス */
  readonly property string imagePath:"qrc:/assets/images/"

  QtObject {
    id: p
    /** 電池残量の塗りつぶし幅 */
    property int  battery_remaining_fill_width : 0
    /** 充電中フラグ */
    property bool is_charging : false
    /** 充電温度範囲外フラグ */
    property bool is_temperature_anomaly : false
    /** アイコン点滅中 */
    property bool is_flashing_start: false
    /** 電池残量の塗りつぶし表示 色 */
    property color remaining_color: "#cccccc"

    /** 電池残量の塗りつぶし表示 低残量閾値 */
    readonly property int low_remaining_threshold: 10
    /** 電池残量の塗りつぶし表示 低残量時の塗りつぶし幅 */
    readonly property int battery_low_remaining_fill_width : 3 // 最大塗りつぶし幅 30 * 電池残量(残量10%以下は10固定)10 / 100%
  }

  onParamChanged: {
    if(param[1] <= p.low_remaining_threshold) {
      p.remaining_color = "#ff0000"
      p.battery_remaining_fill_width = p.battery_low_remaining_fill_width
    } else {
      p.remaining_color = "#cccccc"
      p.battery_remaining_fill_width = 30 * param[1] / 100     // 最大塗りつぶし幅 * 電池残量 / 100%
    }
    p.is_charging            = param[2]
    p.is_temperature_anomaly = param[3]
  }

  onAnimationImageChanged: {
    if( animationImage.length > 0 ) {
      temperature_anomaly.source = imagePath + animationImage
    } else {
      temperature_anomaly.source = ""
    }
  }

  Image {
    // 電池画像
    id: battery_guage
    anchors.left: parent.left
    source: "qrc:/assets/images/stat_sys_battery_guage_ic_flame.png"
    Rectangle {
      // 電池残量の塗りつぶし表示
      id:background
      anchors.verticalCenter: parent.verticalCenter
      anchors.right: parent.right
      anchors.rightMargin: 6
      height: 12
      color: p.remaining_color
      width: p.battery_remaining_fill_width
    }
  }
  Image {
    // 充電中マーク
    id: charging
    anchors.fill: parent
    visible: p.is_charging
    source: "qrc:/assets/images/stat_sys_battery_guage_ic_charging.png"
  }
  Image {
    // 充電温度範囲外マーク
    id: temperature_anomaly
    anchors.fill: parent
    visible: p.is_temperature_anomaly
    source: ""
  }
}

//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"

ScreenBase {

  Rectangle {
    anchors.fill: parent
    color: viewstyle.bgcolor.D1
  }

  /**
   * Dac Filter 選択時シグナル
   * @param 選択ボタン番号
   */
  signal notifySetDacFilterIndex(int filter_num)

  id: dac_filter_setting_screen
  // 選択 DAC Filter Index
  property int dac_filter_index
  property int first_pressed_position_y: -1
  readonly property int flickable_area_max_height: 612
  readonly property int visible_animation_duration: 100
  readonly property int invisible_animation_duration: 0

  // タイトル
  ScreenTitleArea {
    id: titleArea

    // DACフィルター選択
    titleText: qsTr("110019") + viewstyle.empty_string
    visibleMusicLibraryTopicon: false
  }

  ExclusiveGroup {
    id: filter_choose_group
  }

  Column {
    anchors.top: titleArea.bottom
    Repeater {
      model: [QT_TR_NOOP("110021"), QT_TR_NOOP("110023"), QT_TR_NOOP("110025"), QT_TR_NOOP("110027"), QT_TR_NOOP("110029"), QT_TR_NOOP("110031")]
      SettingItemRadio {
        title: qsTr(modelData) + viewstyle.empty_string
        exclusiveGroup: filter_choose_group
        checked: (dac_filter_index === index)
        onClicked: {
          if(!checked)
          {
            notifySetDacFilterIndex(index)
          }
        }
      }
    }
  }
}

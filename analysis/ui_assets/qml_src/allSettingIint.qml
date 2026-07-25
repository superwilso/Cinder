//
// Copyright 2015-2018 Sony Corporation
//

import QtQuick 2.3
import "qrc:/components"
import "qrc:/parts"

Rectangle {
  id: allSettingIint
  color: "black"

  property int scrollRightMargin: 4
  /** type:int SD カードのスロット数 */
  property int sd_card_slot_num
  /** type:string SD SDカードの挿入状態 */
  property string sd_card_state_info
  /** type:bool SDカードが挿入されているか否か */
  property bool has_sd_card

  signal startSettingInit()
  signal startFactoryReset()
  signal startFormatInternalMemory()
  signal startFormatSDMemory()
  signal startReUpdateDB()
  /**
   * Dual　SD 以上用の SD 初期化選択通知
   */
  signal selectSdInitializeSetting()

  ScreenTitleArea {
    id: titleArea
    z: 4
    titleText: qsTr("010049") + viewstyle.empty_string
    visibleMusicLibraryTopicon: false
    fontWeight: Font.Light
  }

  Flickable {
    id: allSettingInitFlickable
    width: 480
    height: allSettingIint.height - titleArea.height
    boundsBehavior: Flickable.StopAtBounds
    y: titleArea.height
    contentHeight: initSettingItems.height
    interactive: false

    Column {
      id: initSettingItems

      // 設定初期化
      SettingItemNormal {
        title: qsTr("010050") + viewstyle.empty_string
        onClicked: startSettingInit()
      }

      // 本体メモリー初期化
      SettingItemNormal {
        title: qsTr("010051") + viewstyle.empty_string
        onClicked: startFormatInternalMemory()
      }

      // SDカード初期化(Single SD 用)
      SettingItemNormal {
        title: qsTr("010052") + viewstyle.empty_string
        description: sd_card_state_info
        enable: has_sd_card
        onClicked: startFormatSDMemory()
        visible: sd_card_slot_num === 1
      }

      // SDカード初期化(Dual SD 以上用)
      SettingItemNormal {
        title: qsTr("010052") + viewstyle.empty_string
        onClicked: selectSdInitializeSetting()
        visible: sd_card_slot_num >= 2
      }

      // データベース再構築
      SettingItemNormal {
        title: qsTr("010053") + viewstyle.empty_string
        onClicked: startReUpdateDB()
      }

      // 工場出荷時状態に戻す
      SettingItemNormal {
        title: qsTr("010054") + viewstyle.empty_string
        onClicked: startFactoryReset()
      }
    }
  }

  SCmnScrollIndicator {
    anchors { top: allSettingInitFlickable.top; bottom: allSettingInitFlickable.bottom;
              right: allSettingInitFlickable.right; rightMargin: scrollRightMargin; }
    z: 2
    flickable: allSettingInitFlickable
    visible: (initSettingItems.height > (allSettingIint.height - titleArea.height))
  }
}

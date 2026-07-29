//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
  color: viewstyle.bgcolor.D1

  //! 公開プロパティ
  /** type:int 再生品質の設定値 */
  property int player_quaity_setting : 0

  /**
   * 設定項目タップ時シグナル
   * @param value 設定値
   */
  signal selectBtReceiverPrioritySetting(int value)

  // リスト項目
  ListModel {
    id: repeaterItem
    ListElement {
      name : QT_TR_NOOP("230087") //priority_to_quality
    }
    ListElement {
      name : QT_TR_NOOP("230088") // priority_to_connection
    }
  }

  /**
   * タイトル
   */
  ScreenTitleArea{
    id: titleArea
    titleText: qsTr("230085") + viewstyle.empty_string
    visibleMusicLibraryTopicon : false
  }

  Item {
    id: settingArea
    anchors.top: titleArea.bottom
    width: 480
    height: columnItems.height
    ExclusiveGroup {
      id: priorityGroup
    }

    Column {
      id: columnItems
      Repeater {
        model: repeaterItem
        // 優先設定
        SettingItemRadio {
          exclusiveGroup: priorityGroup
          title: qsTr(name) + viewstyle.empty_string
          checked: (index == player_quaity_setting)
          onClicked: {
            selectBtReceiverPrioritySetting(index)
          }
        }
      }
    }
  }
}

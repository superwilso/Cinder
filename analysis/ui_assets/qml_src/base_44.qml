//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/components"
import "qrc:/parts"
import "qrc:/framework"
import "qrc:/"

SdSettingBase
{
  id: base

  /** type:var 表示するリスト */
  property var list_model

  /* マウント/アンマウント開始選択通知 */
  signal startMountOrUmount(int index)

  title_string: qsTr("010028") + viewstyle.empty_string
  list_data: list_model
  delegate_component:
  Component
  {
    SettingItemNormal
    {
      width: base.width
      title: first_label
      description: second_label
      onClicked: startMountOrUmount(index)
      enable: enable_state
      visible: true
    }
  }
}

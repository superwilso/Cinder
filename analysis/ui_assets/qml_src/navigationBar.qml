//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.0
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/components/menu"
import "qrc:/framework"

Rectangle
{
    id: navigationBar

    color: "black"

    width: parent.width
    height: 72

    signal back()
    signal play()
    signal library()
    signal option(int index)

    property var properties
    property var optionMenuModel: properties["option"]
    property bool optionMenuOpened: false
    property alias backButtonEnabledState: backButton.enabled
    property alias optionMenuDialog: dialog

    objectName: properties["objectName"]

    MouseArea {
        anchors.fill: parent
    }

    StateButton {
      id: backButton
      x: 0
      width: parent.width / 4; height: parent.height

      objectName: "BackButton"

      properties: navigationBar.properties[objectName]

      onClicked: {
          back()
      }
    }
    StateButton {
      x: 120
      width: parent.width / 4; height: parent.height

      objectName: "NowPlayingButton"

      properties: navigationBar.properties[objectName]

      onClicked: {
          play()
      }
    }
    StateButton {
      x: 240
      width: parent.width / 4; height: parent.height

      objectName: "FunctionTopButton"

      properties: navigationBar.properties[objectName]

      onClicked: {
        library()
      }
    }
    StateButton {
      x: 360
      width: parent.width / 4; height: parent.height

      objectName: "OptionButton"

      properties: navigationBar.properties[objectName]

      onClicked: {
        if (false == optionMenuOpened){
            // メニュー未表示の場合のみ表示指示を行う
            if("NoneDialog" === dialog.dialog) {
                dialog.dialogOpen("kMid9211", "", "", optionMenuModel)
            } else {
                dialogManager.forceClose()
            }
        }
        else if(true == optionMenuOpened){
            dialogManager.forceClose()
        }
        else {}
      }
    }

    // WindowのNavigationBar: optionMenuModel が変化した＝画面遷移した               => オプションメニューが開いていたら閉じる
    // DialogのNavigationBar: optionMenuModel が変化した＝オプションメニューが開いた => dialog.dialog は空なので何もしない
    onOptionMenuModelChanged: {
      if("NoneDialog" !== dialog.dialog){
        dialog.close()
      }
    }
    onVisibleChanged: {
        if ((visible === false) && ("OptionMenu" === dialog.dialog)) {
            dialog.close()
        }
    }

    DialogWrapper {
        id: dialog
        visible: false
        dialog: "NoneDialog"
        onSelected: {
            option((parseInt)(item_id))
        }
    }
}

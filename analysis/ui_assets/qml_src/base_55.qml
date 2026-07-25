//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
  id:base
  color: "black"
  anchors.fill: parent

  property var infomation     // 表示情報

  readonly property int kListTopMargin  : 12          // rectのTopMargin

  // Title Area
  ScreenTitleArea {
    id: titleArea
    titleText: qsTr("230037") + viewstyle.empty_string   // 機器情報
    visibleMusicLibraryTopicon: false
  }

  // デバイス名が長く画面をはみ出した場合に備えてスクロールできるようにしておく
  SCmnScrollIndicator {
    id: scroll
    anchors {
      top: flickableArea.top
      bottom: flickableArea.bottom
      right: flickableArea.right
      rightMargin: 4
    }
    z: 10
    flickable: flickableArea
    visible: flickableArea.height < flickableArea.contentHeight
  }
  Flickable {
    id: flickableArea
    anchors.top: titleArea.bottom
    width: parent.width
    height: 612  // ステータスバー(12px)＋ナビゲーションバー(88px)＋タイトルエリア(88px)を画面サイズ(800px)から引いた値
    interactive: (contentHeight > height) ? true : false
    boundsBehavior: Flickable.StopAtBounds
    contentHeight: items.height + kListTopMargin
    clip: true
    visible: true
    Column{
      id:items
      anchors { top: parent.top; topMargin: kListTopMargin; }
      height:childrenRect.height
      spacing: 24
      Repeater {
        model:infomation
        Loader {
          id:info
          sourceComponent: infoDelegate
          onLoaded: {
            item.info = infomation[index]
          }
        }
      }
    }
  }
  Component {
    id:infoDelegate
    Item {
      property var info
      id:infoItem
      height:childrenRect.height
      width:childrenRect.width
      anchors.top:parent.top
      anchors.left:parent.left
      Column {
        Repeater {
          model:info
          SCmnLabel {
            id:infoItemTitleText
            width: 440
            anchors.left: parent.left
            font.pixelSize: index !== 0 ? viewstyle.textsize.L : viewstyle.textsize.S
            font.weight: Font.Light
            color: index !== 0 ? viewstyle.textcolor.L1 : viewstyle.textcolor.L2
            anchors.leftMargin: 20
            text:info[index]
            wrapMode: Text.Wrap // 長いとき折り返し指定
          }
        }
      }
    }
  }
}

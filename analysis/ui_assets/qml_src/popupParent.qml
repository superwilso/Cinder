//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"

Item {
  id: popupParent
  /** type int 進捗値 */
  property int currentProgress
  /** type var ナビゲーションバー情報 */
  property var navigationBar
  property var headerArea

  Rectangle {
    id: background
    anchors.fill: parent
    color: viewstyle.bgcolor.D1
  }

  HeaderArea {
      id: header
      anchors { top: parent.top; left: parent.left; }
      properties: headerArea
      Rectangle {
        anchors { top: parent.top; left: parent.left; right: parent.right; }
        height: 54
        color: viewstyle.bgcolor.D1
        opacity: 0.5
        visible: navigationBar.properties["useVolumeArea"]
      }
  }

  SCmnLabel {
    id: message_text
    anchors {
      top: parent.top; left: parent.left; right: parent.right;
      topMargin: 304; leftMargin: 20; rightMargin: 20
    }
    height: message_text.font.pixelSize
    verticalAlignment: Text.AlignVCenter
    horizontalAlignment: Text.AlignHCenter
    color: viewstyle.textcolor.L1
    font.pixelSize: viewstyle.textsize.LL
    text: qsTr("010000") + viewstyle.empty_string
  }
  Image {
    id: progress_base
    width: sourceSize.width
    height: sourceSize.height
    anchors {
      top: message_text.bottom
      topMargin: 120
      horizontalCenter: parent.horizontalCenter
    }
    source: "qrc:/common_full_progerss_first.png"
    Image {
      id: progress
      width: sourceSize.width * (currentProgress / 100)
      height: sourceSize.height
      fillMode: Image.Tile
      anchors { top: parent.top; left: parent.left; }
      source: "qrc:/common_full_progerss_complete.png"
    }
  }
  NavigationBar {
      id: footer
      anchors { bottom: parent.bottom; left: parent.left; }
      properties: navigationBar
  }

  // マウスイベント盗難防止
  MouseArea {
      anchors.fill: parent
      preventStealing: true
  }
}

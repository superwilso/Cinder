//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "../components"

Item {
  id: usbDacDeviceWindow
  visible: true
  property var headerArea
  property var navigationBar
  property var pageIndicator
  property var sourceDirectOrClearAudio

  // スワイプ感知領域の拡大対応
  MouseArea {
    id: headerArea

    anchors.top: parent.top
    width: parent.width
    height: header.height
  }

  HeaderArea { // 44
    id: header
    anchors.top: parent.top
    properties: usbDacDeviceWindow.headerArea
  }

  SourceDirectOrClearAudioComp {
    id: directSound
    anchors { top: header.bottom; right: parent.right; }
    properties: usbDacDeviceWindow.sourceDirectOrClearAudio
  }

  // スワイプ感知領域の拡大対応
  // ページインジケータは、MouseAreaを内包しないエレメントのため、範囲に含めない
  // 当MouseAreaの高さを適切に変更しないと、Screenの高さが変更されない状態となるため、注意が必要。
  MouseArea {
    id: footerArea

    anchors.bottom: parent.bottom
    width: parent.width
    height: (footer.visible && footer.opacity === 1.0) ? footer.height : 0
  }

  PageIndicator {
    id: pageindicator
    anchors { bottom: footer.top; bottomMargin: 10; horizontalCenter: parent.horizontalCenter; }
    properties: usbDacDeviceWindow.pageIndicator
  }

  NavigationBar { // 56
    id: footer
    anchors.bottom: parent.bottom
    properties: usbDacDeviceWindow.navigationBar
  }

  // スワイプ感知領域の拡大対応
  SwipeSelector {
    id: headerSwipeSelector
    objectName: "headerSwipeSelector"

    anchors.fill: headerArea
  }

  SwipeSelector {
    id: footerSwipeSelector
    objectName: "footerSwipeSelector"

    anchors.fill: footerArea
  }
}

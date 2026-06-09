//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/components"
import "qrc:/parts"
import "qrc:/framework"
import "ContentsBrowseCommon.js" as Script

Item {
  id: screen

  property var highResolutionList
  property string title

  signal touchListAt(int item_id, string item_text)

  QtObject {
    id: p
    property int titleHeight: 88
    property int cellHeight: 84
    property int areaSpacing: 20
    property int contentSpacing: 10
    property int longContentsWidth: 356
    property int shortContentsWidth: 80
    property int verticalMargin: 10
    property url main_separator_image: "qrc:/assets/images/separator_main.png"
  }

  Rectangle {
    id: background
    anchors.fill: parent
    color: viewstyle.bgcolor.D1
  }

  EdgeDetectListView {
    id: listView
    anchors { top: title_area.bottom; left: screen.left; bottom: bottom_main_separator.top; }
    width: screen.width
    model: highResolutionList

    delegate: Item {
      width: listView.width
      height: p.cellHeight + separator.height
      Image {  // Press画像は、表示時には必須エレメントではないため、遅延して表示させる。
        asynchronous: true
        anchors { top: parent.top; left: parent.left; }
        visible: contents.isPressed
        source: visible? "qrc:/list_full_pressed.png" : ""
      }
      SCmnButton {
        id: contents
        anchors { top: parent.top; left: parent.left; right: parent.right; }
        height: p.cellHeight
        property string item_name: item_text
        property bool isPressed: false
        loadComponent: Item {
          anchors.fill: parent
          Image {
            anchors { top: parent.top; left: parent.left }
            height: parent.height; width: parent.height
            fillMode: Image.PreserveAspectFit; smooth: true
            source: icon_path
          }
          SCmnLabel {
            id: name_text
            anchors { top: parent.top; left: parent.left; leftMargin: p.areaSpacing + parent.height; }
            height: p.cellHeight
            width: p.longContentsWidth
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
            font.pixelSize: viewstyle.textsize.L
            font.weight: Font.Light
            clip: true
            text: contents.item_name
            Image {  // 消し込み画像は、表示時には必須エレメントではないため、遅延して表示させる。
              asynchronous: true
              anchors { top: parent.top; right: parent.right; bottom: parent.bottom; }
              height: parent.height
              visible: (parent.width - 48 >= parent.implicitWidth)? false : true
              source: visible? (contents.isPressed ? "qrc:/ic_list_item_mask_right_pressed.png" : "qrc:/ic_list_item_mask_right.png") : ""
            }
          }
        }
        pressedComponent: loadComponent
        onEdgeReleased: listView.onReleasedEvent()
        onEdgeCanceled: listView.onReleasedEvent()
        onPressedYPosition: listView.setFirstPressedYPosition(posY)
        onMovedYPosition: listView.movingYPosition(posY)
        onClicked: {
          touchListAt(item_id, contents.item_name)
          contents.isPressed = false
        }
        onPressed: {
          contents.isPressed = true
        }
        onReleased: {
          contents.isPressed = false
        }
      }
      Image {
        id: separator
        anchors { top: contents.bottom; left: parent.left; }
        source: "qrc:/assets/images/separator_sub3.png"
      }
    }
  }
  Item {
    id: title_area
    anchors { top: parent.top; left: parent.left; right: parent.right; }
    height: p.titleHeight + main_separator.height
    visible: false
    SCmnLabel {
      id: title_text
      anchors
      {
        top: parent.top;
        left: parent.left;
        right: parent.right;
        bottom: main_separator.top;
        leftMargin: p.areaSpacing;
        rightMargin: p.areaSpacing;
      }
      verticalAlignment: Text.AlignVCenter
      horizontalAlignment: Text.AlignLeft
      font.pixelSize: viewstyle.textsize.L
      font.weight: Font.Light
      clip: true
      text: title
      Image {
        anchors { top: parent.top; right: parent.right; }
        source: "qrc:/ic_music_library_list_title_mask_right.png"
      }
    }
    Image {
      id: main_separator
      anchors { bottom: parent.bottom; left: parent.left; }
      source: p.main_separator_image
    }
  }
  Image {
    id: bottom_main_separator
    anchors { bottom: parent.bottom; left: parent.left; }
    source: p.main_separator_image
  }
  Rectangle {
    id: blind
    anchors.fill: parent
    color: viewstyle.bgcolor.D1
    opacity: 1
  }
  states: [
    State {
      name: "DataExists"
      when: listView.count > 0
      PropertyChanges { target: blind; opacity: 0; }
      PropertyChanges { target: title_area; visible: true; }
    }
  ]

  Connections {
    target: controller
    onUnavailable: {
      screen.list = 0
    }
  }
}

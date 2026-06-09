//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"

Item {
  id: dialogRoot

  // シグナル
  /**
   * 項目選択時シグナル
   * @param item_id 選択行の ID
   */
  signal selected(var item_id)
  /**
   * accept 通知シグナル
   * @param Nothing
   */
  signal accepted()

  QtObject {
    id: p
    property int upper_lower_radius: 4   // 上下丸みエリア用高さ
    property int list_height:        88
    property int list_width:         361
    property int separator_height:   1   // セパレータ高さ
    property url popup_area_center:  "qrc:/assets/images/popup_area_center_4r.png"
    property url context_normal:     "qrc:/assets/images/list_context_4s_no_separator_normal.png"
    property url context_pressed:    "qrc:/assets/images/list_context_4s_no_separator_pressed.png"
    property url context_separator:  "qrc:/assets/images/separator_context.png"
  }
  ExclusiveGroup { id: radioGroup }

  Component.onCompleted: {
    list.positionViewAtIndex(list.checkIndex, ListView.End)
  }
  // リストダイアログ
  Rectangle {
    id: dialog
    color: "#00000000"      // 透過指定
    width: p.list_width
    height: {
      if (topText.visible)
      {
        // タイトルエリア表示中は、下の丸みエリア分増やす
        topText.height + listArea.height + p.upper_lower_radius
      }
      else
      {
        listArea.height  + (p.upper_lower_radius * 2)
      }
    }
    anchors.centerIn: parent
    radius: p.upper_lower_radius

    BorderImage {
      id: centerImage
      anchors.fill: parent
      source: p.popup_area_center   // 361 x 90
      border.left: p.upper_lower_radius; border.top: p.upper_lower_radius
      border.right: p.upper_lower_radius; border.bottom: p.upper_lower_radius
    }

    // 上部テキスト
    TopText {
      id: topText
    }
    // リスト
    Item {
      id: listArea
      width: parent.width
      height: {
        if (list.count >= 5)
        {
          Math.floor((p.list_height + p.separator_height) * 4.5)         // (88 + 1) * 4.5 = 400
        }
        else
        {
          (p.list_height + p.separator_height) * list.count - p.separator_height
        }
      }
      anchors.top: topText.visible === true ? topText.bottom : parent.top
      anchors.topMargin: topText.visible === true ? 0 : p.upper_lower_radius
      clip: true

      ListView {
        id: list
        anchors.fill: parent
        model: dmpDialog.list_model
        delegate: delegate
        boundsBehavior: Flickable.StopAtBounds
        clip: true
      }
      Item {
        width: 20
        anchors { top: list.top; bottom: list.bottom; right: list.right; }
        SCmnScrollIndicator {
          id: scroll
          anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; bottom: parent.bottom; }
          flickable: list
        }
      }
      Component {
        id: delegate
        Item {
          height: (index === list.count-1) ? p.list_height : (p.list_height + p.separator_height)
          width: list.width
          SCmnButton {
            anchors { top: parent.top; left: parent.left; right: parent.right; }
            height: p.list_height
            loadComponent: Item {
              anchors.fill: parent
              Image {
                anchors.fill: parent
                source: p.context_normal
              }
              SCmnLabel {
                anchors { fill: parent; leftMargin: 20; rightMargin: 20 }
                text: label
                enabled: enable
                font.pixelSize: viewstyle.textsize.L
                font.weight: Font.Light
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignLeft
                verticalAlignment: Text.AlignVCenter
              }
            }
            pressedComponent: Item {
              anchors.fill: parent
              Image {
                anchors.fill: parent
                source: p.context_pressed
              }
              SCmnLabel {
                anchors { fill: parent; leftMargin: 20; rightMargin: 20 }
                text: label
                enabled: enable
                font.pixelSize: viewstyle.textsize.L
                font.weight: Font.Light
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignLeft
                verticalAlignment: Text.AlignVCenter
                }
            }
            onClicked: {
              if (model.item_id !== undefined)
              {
                dialogRoot.selected(model.item_id)
                dialogRoot.accepted()
              }
            }
          }
          // セパレータ
          // ダイアログ下部は、角部分にradiusが設定されているため、
          // リストの範囲は、ダイアログ全体にたいして若干小さめとなっている。
          // セパレータを下部に配置すると、下側にセパレータが表示されるため、最終行に対しては、セパレータは設定しない。
          Image {
            anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
            visible: (index === list.count-1) ? false : true
            height: p.separator_height
            source: p.context_separator
          }
        }
      }
    }
  }
}

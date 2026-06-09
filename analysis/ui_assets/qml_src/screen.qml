//
// Copyright 2017-2018 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Window 2.2
import "qrc:/components"
import "qrc:/parts"
import "qrc:/framework"
import "ContentsBrowseCommon.js" as Script

Item {
  id: screen

  property var list
  property variant initial_char_list
  property string title
  property int selected_index: 0
  property string invalid_main_meta_from_db: ""
  property bool is_exist_initial_char: false

  signal touchListAt(int index, string item_text)
  signal createContextMenu(int list_index, string contents_name)

  signal setScrollPositionResume
  onSetScrollPositionResume: {
    p.needScrollPositionChange = true;
    p.lastContentY = listView.contentY;
  }
  signal resetScrollPositionResume
  onResetScrollPositionResume: {
    p.needScrollPositionChange = false;
    p.lastContentY = 0;
  }
  signal requestUpdateSelectedIndex(int index)
  onRequestUpdateSelectedIndex: {
    selected_index = index
  }

  QtObject {
    id: p
    property int titleHeight: 88
    property int cellHeight: 84
    property int separator_height: 1
    property int areaSpacing: 20
    property int display_margin_beginning: Math.floor((p.cellHeight+p.separator_height) * Script.get_NotExistJacket_DisplayMarginCount())
    property int display_margin_end: Math.floor((p.cellHeight+p.separator_height) * Script.get_NotExistJacket_DisplayMarginCount())

    property bool needScrollPositionChange: false
    property real lastContentY: 0
    property string unknown_string: qsTr("000006") + viewstyle.empty_string
    property url main_separator_image: "qrc:/assets/images/separator_main.png"
  }

  // Back時の表示位置補正処理
  onSelected_indexChanged: {
    listView.contentY = Script.setInitialPosition(p.needScrollPositionChange, selected_index, p.cellHeight + p.separator_height, listView.contentHeight, listView.height, p.lastContentY)
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
    model: list
    // NoEdit：タイトル部分に滑り込む対応が必要な際には、下記を有効にする必要あり。
    // ただし、消し込み画像のデザインによっては、リスト上部のバッファ量の調整が必要。
    // displayMarginBeginning: Math.floor(p.cellHeight * 1.1)
    displayMarginBeginning: p.display_margin_beginning
    displayMarginEnd: p.display_margin_end
    maximumFlickVelocity: Script.get_NotExistJacket_MaximumFlickVelocity()
    flickDeceleration: Script.get_NotExistJacket_FlickDeceleration()
    stop_animation_duration: Script.get_NotExistJacket_StopAnimationDuration()
    onModelChangedOrReset: {
      if (!model) return
      contentY = Script.setInitialPosition(p.needScrollPositionChange, selected_index, p.cellHeight + p.separator_height, contentHeight, height, p.lastContentY)
    }
    onCountChanged: {
      if (count <= 0) return
      if (screen.state != "DataExists")
      {
        // 件数が1件以上ある場合は、以降ブラインドしない（削除、移動により0件となった場合でもブラインドしないことで、タイトル部を表示し続ける）
        screen.state = "DataExists"
      }
    }

    delegate: Item {
      width: listView.width
      height: 85
      Image {
        id: pressed_background
        anchors { top: parent.top; left: parent.left; }
        visible: false
        enabled: false
        source: ""
      }
      SCmnContextMenuWithSubjectForContentsList {
        id: contents
        anchors { top: parent.top; left: parent.left; right: parent.right; }
        height: 84
        visible: false
        opacity: 0
        need_edge_detect: listView.need_edge_detect
        property bool   is_show: false
        property bool   is_pressed: false
        property bool   is_exist_contents_cache: IsExistContentsCacheRole
        property string item_text: ""
        property string contents_count_text: ""
        property string fade_image_top: ""
        loadComponent: Item {
          anchors.fill: parent
          SCmnLabel {
            id: main_name
            anchors { top: parent.top; topMargin: 8; left: parent.left; leftMargin: 20; }
            height: 36
            width: 392
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
            font.pixelSize: viewstyle.textsize.L
            font.weight: Font.Light
            clip: true
            text: contents.item_text
            onImplicitWidthChanged:
            {
              fade_top.visible = (344 >= main_name.implicitWidth) ? false : true
              contents.fade_image_top = contents.is_pressed ? "qrc:/ic_list_item_mask_right_pressed.png" : "qrc:/ic_list_item_mask_right.png"
            }
            Image {
              id: fade_top
              anchors { top: parent.top; right: parent.right; bottom: parent.bottom; }
              height: 36
              visible: false
              enabled: false
              source: contents.fade_image_top
            }
          }
          SCmnLabel {
            id: contents_count
            anchors { top: main_name.bottom; left: main_name.left; }
            height: 32
            width: 80
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
            font.pixelSize: viewstyle.textsize.M
            font.weight: Font.Light
            normal_color: viewstyle.textcolor.L2
            text: contents.contents_count_text
          }
        }
        onEdgeReleased: listView.onReleasedEvent()
        onEdgeCanceled: listView.onReleasedEvent()
        onPressedYPosition: listView.setFirstPressedYPosition(posY)
        onMovedYPosition: listView.movingYPosition(posY)
        onClicked: {
          if (!is_show) return
          touchListAt(index, item_text)
          contents.is_pressed = false
        }
        onPressed: {
          // Press画像が表示されないようにするため、Pressイベントにも判定追加
          if (!is_show) return
          contents.is_pressed = true
        }
        onReleased: {
          contents.is_pressed = false
        }
        // コンテキスト部分がクリック
        onContextClicked: {
          if (!is_show) return
          setScrollPositionResume()
          createContextMenu(index, contents.item_text)
        }
        // コンテキスト以外の部分がロングプレス
        onPressAndHold: {
          if (!is_show) return
          setScrollPositionResume()
          createContextMenu(index, contents.item_text)
          contents.is_pressed = false
        }
        onIs_pressedChanged:
        {
          pressed_background.visible = is_pressed
          pressed_background.enabled = is_pressed
          pressed_background.source = is_pressed ? "qrc:/list_full_pressed.png" : ""
          contents.fade_image_top = is_pressed ? "qrc:/ic_list_item_mask_right_pressed.png" : "qrc:/ic_list_item_mask_right.png"
        }
        onIs_exist_contents_cacheChanged:
        {
          if (!is_exist_contents_cache)
          {
            return
          }

          is_show = true
          setValues()
          changeState(listView.is_flick_control, listView.is_below_low_speed_threshold)
        }
        function setValues()
        {
          item_text = NameQstringRole
          if (item_text === invalid_main_meta_from_db)
          {
            item_text = p.unknown_string
          }

          // IsExistContentsCacheRole = true になったからと言って、すぐに取得できるわけではない情報は代入にすることはできない
          // バインドでないと　C++ から dataChanged() が上がっても、 QML で受けることができない
          // コンテンツ内件数は、そこを掘ったと仮定して IdList を取得してその長さを返しているので、別途検索する必要がある
          // 例えば、下記のようにしてバインドしてあげると動作させることができる
          contents_count_text = Qt.binding(function() { return ContentsNumUint32Role })
        }
      }
      Rectangle {
        id: separator
        anchors { top: contents.bottom; left: parent.left; }
        color: "#333333"
        width: 480
        height: 1
      }
    }
  }
  SCmnScrollBarForContentsList {
    id: scroll
    flickable: listView
    initial_char_list: screen.initial_char_list
    anchors.fill: listView
    balloonShow: is_exist_initial_char
  }

  Item {
    id: title_area
    anchors { top: parent.top; left: parent.left; right: parent.right; }
    height: p.titleHeight + main_separator.height
    visible: false
    // NoEdit：タイトル部分への滑り込み対応が必要な際に、下記を有効にする必要あり。
    // 操作無効化用マウスエリア
    // MouseArea {
    //   anchors.fill: parent
    // }
    // Image {
    //   id: upper_mask
    //   anchors { top: parent.top; left: parent.left; }
    //   source: "qrc:/ic_list_full_mask_upper.png"
    //   height: sourceSize.height / 2
    // }
    Image {
      id: main_separator
      anchors { bottom: title_area.bottom; left: parent.left; }
      source: p.main_separator_image
    }
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
      onImplicitWidthChanged:
      {
        title_text_fade.visible = (width - 48 >= title_text.implicitWidth) ? false : true
      }
      Image {
        id: title_text_fade
        anchors { top: parent.top; right: parent.right; }
        visible: false
        onVisibleChanged:
        {
          source = visible ? "qrc:/ic_music_library_list_title_mask_right.png" : ""
        }
        source: ""
      }
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
    enabled: false
  }
  states: [
    State {
      name: "DataExists"
      PropertyChanges { target: blind; opacity: 0; }
      PropertyChanges { target: blind; visible: false; }
      PropertyChanges { target: title_area; visible: true; }
    }
  ]

  Connections {
    target: controller
    onUnavailable: {
      screen.state = ""
      screen.list = 0
      screen.selected_index = 0
    }
  }
}

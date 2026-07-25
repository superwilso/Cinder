//
// Copyright 2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
  color: viewstyle.bgcolor.D1

  //! 公開プロパティ
  /** type:var 設定一覧 */
  property var registered_devices

  // ViewModelへのSignal通知
  /**
   * 機器情報一覧選択
   */
  signal selectBtReceiverDeviceListItem(int index)

  QtObject {
    id: p
    readonly property int kListRowHeight         : 84  // リスト高さ
    readonly property int kLeftMargin            : 20  // screen の左マージン(共通)
    readonly property int kScrollIndicatorMargin : 10  // scrollIndicator表示位置
  }

  /**
   * タイトル
   */
  ScreenTitleArea{
    id: titleArea
    titleText: qsTr("230086") + viewstyle.empty_string
    visibleMusicLibraryTopicon : false
  }
  /**
   * 設定一覧
   */
  ListView {
    id: deviceListView
    anchors {
      top: titleArea.bottom
      bottom: parent.bottom
      left: parent.left
    }
    width: parent.width
    clip: true
    model: registered_devices
    delegate: deviceListRow
    spacing: 0
    boundsBehavior: Flickable.StopAtBounds
    snapMode: ListView.SnapToItem
    Item{
      id:pairedScrollArea
      width:p.kScrollIndicatorMargin
      anchors { top: parent.top; bottom: parent.bottom; right: parent.right; }
      SCmnScrollIndicator {
        id: scroll
        anchors { top: parent.top; bottom: parent.bottom; horizontalCenter: parent.horizontalCenter; }
        flickable: deviceListView
      }
    }
  }
  Component {
    id: deviceListRow
    Item {
      width: parent.width
      height: p.kListRowHeight
      property string deviceName: DeviceNameQstringRole === "" ? DeviceMacAddressQstringRole : DeviceNameQstringRole
      property bool isButtonPressed: false

      Image {
        anchors.fill: parent
        visible: isButtonPressed
        source: "qrc:/list_full_pressed.png"
      }
      SCmnContextMenuWithSubject {
        id: itemArea
        anchors.fill: parent
        separatorVisible: false
        isEnablePressAndHold: true

        loadComponent: Item {
          anchors.fill: parent
          SCmnLabel {
            id: registeredDeviceName
            anchors { left: parent.left; leftMargin: p.kLeftMargin; }
            width: 376
            height: DeviceConnectStatusQstringRole === "" ? parent.height : parent.height / 2
            text: deviceName
            color: viewstyle.textcolor.L1
            verticalAlignment: DeviceConnectStatusQstringRole === "" ? Text.AlignVCenter :Text.AlignBottom
            font.pixelSize: viewstyle.textsize.L
            font.weight: Font.Light
            elide: Text.ElideRight
          }
          SCmnLabel {
            id:registeredDeviceConnectStatus
            anchors { left: parent.left; leftMargin: p.kLeftMargin; top: registeredDeviceName.bottom; }
            width: 376
            height: parent.height / 2
            text: DeviceConnectStatusQstringRole
            color: viewstyle.textcolor.L2
            verticalAlignment: Text.AlignTop
            font.pixelSize: viewstyle.textsize.S
            font.weight: Font.Light
          }
          Image {
            anchors.bottom: parent.bottom
            source: "qrc:/assets/images/separator_sub2.png"
          }
        }

        onClicked: {
          selectBtReceiverDeviceListItem(index)
        }
        onPressed: {
          isButtonPressed = true
        }
        onReleased: {
          isButtonPressed = false
        }
        // コンテキスト部分がクリック
        onContextClicked: {
          selectBtReceiverDeviceListItem(index)
          isButtonPressed = false
        }
        // コンテキスト以外の部分がロングプレス
        onPressAndHold: {
          selectBtReceiverDeviceListItem(index)
          isButtonPressed = false
        }
      }
    }
  }
}

//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.0
import QtQml.Models 2.1
import "qrc:/"
import "qrc:/parts"
import "qrc:/framework"
import "qrc:/components"

ScreenBase {
    id: root

    //! 公開プロパティ
    property var icon_list
    property int pageindex
    property bool isAvailableFmRadio
    property bool isAvailableRecorder
    property bool isAvailableLangStudy
    property bool isAvailableDACMode
    /** type:bool BtReceiver機能が有効かどうか */
    property bool is_available_bt_receiver
    property bool isDemoMode
    /** type:var 機能アイコン情報 */
    property var function_icon_info
    /** type:bool BasicPlayerControls が有効かどうか */
    property bool is_available_basic_player_controls: true

    signal touchListAt(int index)
    signal iconListMove(int from, int to)
    signal iconListMoveEnd()
    signal iconListInitialize()

    signal daIconClicked()
    signal recIconClicked()
    signal fmIconClicked()
    signal learningIconClicked()
    signal setpageindex(int index)
    /**
     * @brief Bluetoothレシーバー機能アイコン押下シグナル
     */
    signal receiverIconClicked()

    /**
     * BasicPlayerControls の有無によるレイアウト変更処理
     */
    onIs_available_basic_player_controlsChanged:
    {
      if (is_available_basic_player_controls)
      {
        // height = (画像高さ(88px) + 操作可能拡張領域(0px) + margin(12px)) * rowCount(4個)
        libraryIcon.height = 400
        // 操作可能拡張領域の半分
        libraryIcon.icon_image_top_margin = 0
        libraryIcon.anchors.topMargin = 6

        // topMargin = 画面高さ(800px) - アイコン自体の高さ(20px) - アイコンの下部エレメントへの空白(10px) - アイコンの下部エレメントの総高さ(230px) - ステータスバー(28px)
        pageIndicator.anchors.topMargin = 512
      }
      else
      {
        // height = (画像高さ(88px) + 操作可能拡張領域(26px) + margin(12px)) * rowCount(4個)
        libraryIcon.height = 504
        // 操作可能拡張領域の半分
        libraryIcon.icon_image_top_margin = 13
        libraryIcon.anchors.topMargin = 24

        // topMargin = 画面高さ(800px) - アイコン自体の高さ(20px) - アイコンの下部エレメントへの空白(10px) - アイコンの下部エレメントの総高さ(72px) - ステータスバー(28px)
        pageIndicator.anchors.topMargin = 670
      }
    }

    Rectangle {
        id: background
        anchors.fill: parent
        color: viewstyle.bgcolor.D1
    }

    FunctionIconArea {
        id: function_icon_area
        anchors { top: parent.top; left: parent.left }
        function_icon {
            isAvailableFmRadio: root.isAvailableFmRadio
            isAvailableRecorder: root.isAvailableRecorder
            isAvailableLangStudy: root.isAvailableLangStudy
            isAvailableDACMode: root.isAvailableDACMode
            is_available_bt_receiver: root.is_available_bt_receiver
            function_icon_info: root.function_icon_info
        }
    }

    Image {
        z: 2
        // topMargin = LibraryIcon４つ分(88 * 4) + LibraryIcon同士の隙間３つ(12 * 3)
        //              + FunctionIconとLibraryIconの隙間(6) + FunctioIconとDemoIconの隙間(8)
        //             88 * 4 + 12 * 3 + 6 + 8 = 402
        anchors { top: function_icon_area.bottom; topMargin: 402;
            right: parent.right; rightMargin: 8 }
        source: "qrc:/assets/images/ic_demo_mode_indicator.png"
        visible: isDemoMode
    }

    LibraryIcon {
        id: libraryIcon
        width: 480
        height: 400
        icon_image_top_margin: 0
        anchors
        {
          top: function_icon_area.bottom;
          topMargin: 6
          left: parent.left
        }
        list: icon_list
        pageIndex: root.pageindex
        onTouchAt: {
            touchListAt(index)
        }
        onIconMove: {
            iconListMove(from , to)
        }
        onIconMoveEnd: {
            iconListMoveEnd()
        }
        onPageIndexChanged: {
            if(libraryIcon.pageIndex > -1) {
                setpageindex(libraryIcon.pageIndex)
            }
        }
    }
    function bindingOnListChanged() {
        libraryIcon.bindingOnListChanged()
    }

    PageControl {
        id: pageIndicator
        width: root.width; height: 20
        // BasicPlayerControls の曲タイトル部分がスワイプ開始すると表示/非表示が切り替わるため、
        // bottom 合わせでは上下にずれてしまう
        // そのため top 合わせとする
        anchors { top: parent.top; topMargin: 512; }
        markcount: libraryIcon.pageCount
        nowIndex: libraryIcon.pageIndex
    }

    Connections {
        target: function_icon_area.function_icon
        onDaIconClicked: root.daIconClicked()
        onRecIconClicked: root.recIconClicked()
        onFmIconClicked: root.fmIconClicked()
        onLearningIconClicked: root.learningIconClicked()
        onReceiverIconClicked: root.receiverIconClicked()
    }

    function openIconListInitialDialog(){
        dialog.dialogOpen("kMid9200","", qsTr("020208"), 0) // アイコン表示の初期化をしますか？
    }

    DialogWrapper {
        id: dialog
        onAccepted: {
            iconListInitialize()
        }
    }

    Connections {
        target: controller
        onAvailable: {
            viewstyle.mode = "normal"
        }
    }

    SwipeSelectorForLibrary {
        enabled: !libraryIcon.iconAnimation
        id: swipeSelector
        anchors.fill: libraryIcon
        objectName: "swipeSelector"

        target: libraryIcon
    }
}


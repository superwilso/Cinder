//
// Copyright 2015-2016 Sony Corporation
//

/**
 * @brief スクロールインジケーター
 * インジケーターとして同期するFlickableなコンポーネントをflickableプロパティに設定する。
 * autoHideがTrueの場合、フリックによりflickableコンポーネントが動作した際に、インジケーターが表示され、一定時間動作がない場合、自動で非表示となる。
 */
import QtQuick 2.3
import QtQuick.Controls 1.2

Item {
    id: base
    /**
     * type: Flickable リンク対象となるFlickableコンポーネントを指定します。
     */
    property Flickable flickable: null;
    /**
     * tyle bool スクロールバーを自動的に非表示にするかどうかの設定（デフォルトはtrue）
     */
    property bool autoHide: true
    /**
     * tyle bool インジケータにコントロールがあるかどうか(デフォルトはtrue。true:インジケータ、false:スクロールバー)
     */
    property bool control_exists: true

    opacity: 1
    visible: true

    /**
     * type real スクロールバーの初期表示制御用プロパティ
     */
    property real flickableContentHeight: flickable.contentHeight

    Connections {
        target: controller
        onAvailable: {
            if (control_exists)
            {
              initialShow()
            }
        }
    }

    onFlickableContentHeightChanged: {
      // データ検索を伴うケースでは、Availableタイミングでは、flickable.contentHeightが0のケースがあるため、
      // データがバインドされ、contentHeight決定後にインジケータの初期表示処理を行う。
      if (control_exists)
      {
        initialShow()
      }
    }
    property bool movingVertically: flickable.movingVertically
    onMovingVerticallyChanged: {
      if (control_exists)
      {
        if (movingVertically)
        {
          if (flickableContentHeight != 0 && autoHide)
          {
            state = ""
            hiddenTimer.stop()
          } else {
            if (!autoHide)
            {
              state = ""
              hiddenTimer.stop()
            }
          }
        } else {
          if (flickableContentHeight != 0 && autoHide)
          {
            hiddenTimer.restart()
          } else {
            if (!autoHide)
            {
              state = ""
              hiddenTimer.stop()
            }
          }
        }
      }
    }

    QtObject {
        id: p
        property int minHandleHeight: 20
        property int animationTime: 1000
        property int hideTime: 1000
    }

    BorderImage {
        id: handle
        source: viewstyle.mode==="normal"? "qrc:/scroll_indicator_bar.png" : "qrc:/assets/images/language_study/scroll_language_indicator_bar.png"
        y: (flickable.height - height) * flickable.contentY / (flickable.contentHeight - flickable.height)
        height: setHandleSize()
        border { top: 10; bottom: 10; }
        horizontalTileMode: BorderImage.Stretch
        verticalTileMode: BorderImage.Stretch
        visible: base.opacity===0? false : true
    }

    Timer {
        id: hiddenTimer
        interval: p.hideTime
        repeat: false
        triggeredOnStart: false
        onTriggered: {
            state = "Invisible";
        }
    }
    states: [
      State {
        name: "Invisible"
        PropertyChanges { target: base; opacity: 0; }
        PropertyChanges { target: handle; opacity: 0; }
      },
      State {
        name: "ForceInvisible"
        PropertyChanges { target: base; opacity: 0; }
        PropertyChanges { target: handle; opacity: 0; }
      },
      State {
        name: ""
        PropertyChanges { target: base; opacity: 1; }
        PropertyChanges { target: handle; opacity: 1; }
      }
    ]
    transitions: Transition {
        to: "Invisible";
        PropertyAnimation {
            easing.type: Easing.OutCubic
            target: base
            property: "opacity"
            duration: p.animationTime
        }
        PropertyAnimation {
            easing.type: Easing.OutCubic
            target: handle
            property: "opacity"
            duration: p.animationTime
        }
    }
    /**
     * スクロールインジケータの初期表示処理。
     */
    function initialShow()
    {
      if (flickableContentHeight !== 0 && autoHide)
      {
        state = ""
        hiddenTimer.restart()
      } else if (!autoHide) {
        state = ""
        hiddenTimer.stop()
      }
    }

    /**
     * スクロールバー&スクロールインジケータ強制消去
     */
    function forceInvisible()
    {
      state = "ForceInvisible"
    }

    /**
     * 対象となるFlickableコンポーネントの表示Y座標を変更する。
     */
    function moveFlickableArea() {
        flickable.contentY = (flickable.contentHeight - flickable.height) * handle.y / (flickable.height - handle.height)
    }
    /**
     * スクロールハンドルのサイズを設定する
     */
    function setHandleSize() {
        // visibleArea.heightRatio：フリッカブルエリア全体を1とする表示(見える)部分の割合を表す。
        // 表示部分の領域が存在しない場合、ハンドルサイズを0に設定
        if (flickable.visibleArea.heightRatio === 0) {
            return 0
        // 表示部分の領域が１ページ以内に収まっている場合、ハンドルサイズを0に設定
        } else if (flickable.visibleArea.heightRatio === 1) {
            return 0
        } else {
            // ハンドルが小さすぎる場合は、最小値を設定
            if (flickable.visibleArea.heightRatio * base.height < p.minHandleHeight) {
                return p.minHandleHeight
            } else {
                // 割合変更によりハンドルサイズを返却
                return flickable.visibleArea.heightRatio * base.height
            }
        }
    }
}

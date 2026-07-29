//
// Copyright 2017 Sony Corporation
//

/**
 * @brief スクロールインジケーター (コンテンツブラウズ、ブックマークリスト、再生予定リスト用)
 * インジケーターとして同期するFlickableなコンポーネントをflickableプロパティに設定する。
 * autoHideがTrueの場合、フリックによりflickableコンポーネントが動作した際に、インジケーターが表示され、一定時間動作がない場合、自動で非表示となる。
 */
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/parts"

Item {
    id: base
    /**
     * type: EdgeDetectListView リンク対象となる EdgeDetectListView コンポーネントを指定します。
     */
    property EdgeDetectListView flickable: null;
    /**
     * tyle bool スクロールインジケーターを自動的に非表示にするかどうかの設定（デフォルトはtrue）
     */
    property bool autoHide: true

    opacity: 1
    visible: base.opacity === 0 ? false : true

    /**
     * type real スクロールインジケーターの初期表示制御用プロパティ
     */
    property real flickableContentHeight: flickable.content_height

    QtObject {
      id: p
      property int minHandleHeight: 20
    }

    BorderImage {
      id: handle
      property real calc_y_base: (flickable.list_height - height) / (base.flickableContentHeight - flickable.list_height)

      source: "qrc:/scroll_indicator_bar.png"
      y: flickable.content_y * calc_y_base
      height: {
        // visibleArea.heightRatio：フリッカブルエリア全体を1とする表示(見える)部分の割合を表す。
        // 表示部分の領域が存在しない場合、ハンドルサイズを0に設定
        if (flickable.visibleArea.heightRatio === 0) {
          0
        // 表示部分の領域が１ページ以内に収まっている場合、ハンドルサイズを0に設定
        } else if (flickable.visibleArea.heightRatio === 1) {
          0
        } else {
          // ハンドルが小さすぎる場合は、最小値を設定
          if (flickable.visibleArea.heightRatio * base.height < p.minHandleHeight) {
            p.minHandleHeight
          } else {
            // 割合変更によりハンドルサイズを返却
            flickable.visibleArea.heightRatio * base.height
          }
        }
      }
      border { top: 10; bottom: 10; }
      horizontalTileMode: BorderImage.Stretch
      verticalTileMode: BorderImage.Stretch
      enabled: false
    }

    Timer {
      id: hiddenTimer
      interval: 1000
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
      },
      State {
        name: "ForceInvisible"
        PropertyChanges { target: base; opacity: 0; }
      },
      State {
        name: ""
        PropertyChanges { target: base; opacity: 1; }
      }
    ]
    transitions: Transition {
      to: "Invisible";
      PropertyAnimation {
        easing.type: Easing.OutCubic
        target: base
        property: "opacity"
        duration: 1000
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
}

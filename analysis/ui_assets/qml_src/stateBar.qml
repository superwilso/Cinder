//
// Copyright 2015-2018 Sony Corporation
//
/**
  * @brief ステータスバー
  */

import QtQuick 2.0
import "qrc:/parts"

Rectangle {
    id:stateBar
    width: 480
    height: 28
    color : viewstyle.bgcolor.D1

    /** type:string マウスエリアのenabledプロパティ */
    property bool isEnableStatusBarMouseArea
    /** type:var アイコン情報ListModel    */
    property var stateIconList

    /** ステータスバーエリア クリックシグナル    */
    signal statusBarClicked()

    Row{
        /** type:string icon Qmlファイルのパス    */
        readonly property string iconQmlPath: "icon/"
        /** type:var    icon Qmlファイル名配列 */
        readonly property var iconQml:["","StatusIcon.qml","VolumeIcon.qml","VolumeChangeIcon.qml","BatteryRemainingNumIcon.qml","BatteryRemainingImageIcon.qml"]

        id:row
        Repeater {
            id:repeater
            model:stateIconList
            Loader{
                id:loader
                property int qmlNum:QmlIntRole+0
                height:stateBar.height
                width:ParamIntFrameSize
                source:( 1 <= qmlNum && 5 >= qmlNum ) ? row.iconQmlPath + row.iconQml[qmlNum]:""
                Binding{
                    target:loader.item
                    when:loader.status == Loader.Ready
                    property:'animationImage'
                    value:ParamAnimationImageRole
                }
                Binding{
                    target:loader.item
                    when:loader.status == Loader.Ready
                    property:'param'
                    value:ParamQvariantRole
                }
            }
        }
    }

    // マウスエリア(タップ時のフィードバック表現処理付き)
    ClickArea {
        /** real: 押下表現用エリアの不透過度プロパティ */
        readonly property real feedBackRectOpacity: 0.2
        /** string: 押下表現用エリアのバックグラウンドカラープロパティ   */
        readonly property color feedBackRectBgColor: "#ffffff"
        anchors.fill:parent
        enabled: isEnableStatusBarMouseArea
        onClicked: statusBarClicked()

        Rectangle {
            id: feedBackRect
            anchors.fill: parent
            visible: parent.pressedButton
            opacity: parent.feedBackRectOpacity
            color: parent.feedBackRectBgColor
        }
    }
}

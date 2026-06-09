//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
    id: btAutoConnectSetting
    anchors.fill:parent
    color: "black"

    property int wirelessQualitySet     // ワイヤレス再生品質設定
    property var wirelessQualityName    // ワイヤレス再生品質設定名称
    property bool is_absolute_volume_on // AbsoluteVolumeのOn/Off

    signal goToAutoConnectSetting()
    signal goTowirelessQualitySetting()
    signal absoluteVolumeOnOffToggled()

    Item {
        id: msg
        property string screenTitle              : qsTr("230002") + viewstyle.empty_string    // オーディオ接続設定
        property string wirelessQualityTitle     : qsTr("230009") + viewstyle.empty_string    // ワイヤレス再生品質
        property string absoluteVolumeTitile     : qsTr("230077") + viewstyle.empty_string    // 拡張方式を利用する
        property string absoluteVolumedescription: qsTr("230079") + viewstyle.empty_string    // 音量操作ができない場合は、チェックを入れてください
    }

    Item {
        id: p
        property color bgColor              : "black"           // 背景色

        property int titleHeight            : 84                // タイトルエリア高さ
        property int listHeight1Line        : 84                // リストエリア高さ(1行表示)
        property int listHeight2Line        : 42                // リストエリア高さ(2行表示)
        property int checkBoxHeight         : 84                // チェックボックスエリア高さ

        property int leftMargin             : 20                // 左マージン
    }

    // タイトル
    ScreenTitleArea{
        id: titleArea
        titleText: msg.screenTitle
        visibleMusicLibraryTopicon: false
    }

    SCmnScrollIndicator {
        id: scroll
        anchors {
            top: audioConnectSetting.top
            bottom: audioConnectSetting.bottom
            right: audioConnectSetting.right
            rightMargin: scrollRightMargin
        }
        z: 10
        flickable: audioConnectSetting
    }

    Flickable {
        id: audioConnectSetting
        y: titleArea.height
        width: 480
        height: (flickableAreaMaxHeight <= audioConnectSettingItems.height) ? flickableAreaMaxHeight : audioConnectSettingItems.height
        maximumFlickVelocity: 8000
        contentHeight: audioConnectSettingItems.height
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: audioConnectSettingItems

            ExclusiveGroup {
                id: wirelessQualityGroup
            }

            // ワイヤレス再生品質
            SettingItemNormal {
                title: msg.wirelessQualityTitle
                description: wirelessQualityName
                onClicked: goTowirelessQualitySetting()
            }
            // 拡張方式利用(AbsoluteVolume On/Off)
            SettingItemCheckbox {
                title: msg.absoluteVolumeTitile
                description: msg.absoluteVolumedescription
                checked: is_absolute_volume_on
                onClicked:absoluteVolumeOnOffToggled()
            }
        }
    }
}

//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

ScreenBase {
    id: base
    Rectangle {
        anchors.fill:parent
        color: viewstyle.bgcolor.D1
    }

    property int scrollRightMargin: 4
    property int autoConnectSet         // 自動接続設定
    property var pairedDeviceList       // ペアリング済み機器一覧

    signal selectPairedDeviceSetting(int index)

    Item {
        id: msg
        property string screenTitle     : qsTr("230006") + viewstyle.empty_string    // 起動時自動接続先
    }

    Item {
        id: p
        property int listHeight         : 84        // リストエリア高さ
        property int separatorHeight    : 1         // セパレータエリア高さ
    }

    ExclusiveGroup {
        id:deviceListGroup
    }

    // タイトル
    ScreenTitleArea{
        id: titleArea
        z: 3
        titleText: msg.screenTitle
        visibleMusicLibraryTopicon: false
    }

    ListView {
        id: btAutoConnectSetting
        anchors {
            top: titleArea.bottom
            bottom: parent.bottom
        }
        width: parent.width
        model: pairedDeviceList
        delegate: pairedDelegate
        height: (flickableAreaMaxHeight >= btAutoConnectSetting.contentHeight) ? flickableAreaMaxHeight : flickableAreaMaxHeight.contentHeight
        interactive: (flickableAreaMaxHeight >= btAutoConnectSetting.contentHeight) ? false : true
        boundsBehavior: Flickable.StopAtBounds
    }

    SCmnScrollIndicator {
        id: scroll
        anchors {
            top: titleArea.bottom
            bottom: parent.bottom
            right: btAutoConnectSetting.right
            rightMargin: scrollRightMargin
        }
        flickable: btAutoConnectSetting
    }

    Component {
        id: pairedDelegate
        Rectangle {
            id: pairedItemArea
            height: p.listHeight + p.separatorHeight
            SettingItemRadio{
                title: pairedDeviceList[index]
                exclusiveGroup: deviceListGroup
                checked: {
                    if (index === autoConnectSet) {
                        true
                    } else {
                        false
                    }
                }
                onClicked: {
                    selectPairedDeviceSetting(index)
                }
            }
        }
    }
}

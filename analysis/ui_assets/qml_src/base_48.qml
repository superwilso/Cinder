//
// Copyright 2015-2018 Sony Corporation
//

import QtQuick 2.3
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"
import "qrc:/"

Rectangle {
    id: base
    width: 480
    color: viewstyle.bgcolor.D1

    property bool isBtlGainModeHigh
    property bool isSeGainModeHigh

    signal btlGainModeToggled()
    signal seGainModeToggled()

    ScreenTitleArea {
        id: titleArea
        titleText: qsTr("020237") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
    }

    Column {
        id: column
        y: titleArea.height

        // シングルエンド接続ヘッドホン
        SettingItemCheckbox {
            title: qsTr("020238") + viewstyle.empty_string
            description: qsTr("020239") + viewstyle.empty_string
            checked: isSeGainModeHigh
            onClicked: seGainModeToggled()
        }

        // バランス接続ヘッドホン
        SettingItemCheckbox {
            title: qsTr("020240") + viewstyle.empty_string
            description: qsTr("020241") + viewstyle.empty_string
            checked: isBtlGainModeHigh
            onClicked: btlGainModeToggled()
        }
    }
}

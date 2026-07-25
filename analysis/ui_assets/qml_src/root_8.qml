//
// Copyright 2015-2017 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.1
import QtQuick.Controls.Styles 1.1
import "qrc:/components"
import "qrc:/parts"

Rectangle {
    id: root
    color: viewstyle.bgcolor.D1

    signal goToDateSetting()
    signal goToDateDisplayFormatSetting()
    signal timeDisplayFormatToggled()

    property string dateTime
    property string timeDisplayFormatText
    property string dateDisplayFormatText
    property bool is24HourDisplay

    ScreenTitleArea {
        id: dateSettingTopTitleArea
        z: 11
        titleText: qsTr("010005") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    Column {
        id: column
        y:dateSettingTopTitleArea.height

        // 日付時刻設定
        SettingItemNormal {
            title: qsTr("010006") + viewstyle.empty_string
            description: dateTime
            onClicked: goToDateSetting()
            useFixedFontSet: true
        }

        // 日付表示形式
        SettingItemNormal {
            title: qsTr("010007") + viewstyle.empty_string
            description: dateDisplayFormatText
            onClicked: goToDateDisplayFormatSetting()
        }

        // 24時間表示
        SettingItemCheckbox {
            title: qsTr("010062") + viewstyle.empty_string
            description: timeDisplayFormatText
            checked: is24HourDisplay
            onClicked: timeDisplayFormatToggled()
            useFixedFontSet: true
        }
    }

    DisclaimerGuide {
        anchors {
            top: column.bottom
            left: parent.left
            bottom: parent.bottom
            right: parent.right
        }
        guideText: qsTr("010124") + viewstyle.empty_string
    }
}

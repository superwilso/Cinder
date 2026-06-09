//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/parts"

Rectangle {
    id: base
    visible: true
    color: viewstyle.bgcolor.D1

    property string bitRate
    property string recorderDestMemoryType
    property string synchroSilenceThresholdTime

    signal selectRecorderFormat()
    signal selectRecorderDestMemory()
    signal selectSynchroSilenceThresholdTime()

    ScreenTitleArea {
        id: titleArea
        titleText: qsTr("050000") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    Column {
        id: column
        y: titleArea.height

        // 共通
        SettingItemCategory {
            title: qsTr("050008") + viewstyle.empty_string

            // 録音フォーマット
            SettingItemNormal {
                title: qsTr("050005") + viewstyle.empty_string
                description: bitRate
                onClicked: selectRecorderFormat()
            }

            // 録音先メモリー
            SettingItemNormal {
                title: qsTr("050006") + viewstyle.empty_string
                description: recorderDestMemoryType
                onClicked: selectRecorderDestMemory()
            }
        }

        // シンクロ録音
        SettingItemCategory {
            title: qsTr("050002") + viewstyle.empty_string

            // 曲分割の無音時間
            SettingItemNormal {
                title: qsTr("050007") + viewstyle.empty_string
                description: synchroSilenceThresholdTime
                onClicked: selectSynchroSilenceThresholdTime()
            }
        }
    }
}

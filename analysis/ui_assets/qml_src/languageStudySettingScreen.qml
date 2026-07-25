//
// Copyright 2015-2017 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import QtQuick.Controls.Styles 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/"

Rectangle {
    id: languageStudySettingScreen
    color: viewstyle.bgcolor.D1

    signal shuffleOnOffToggled()
    signal setRepeatMode(int repeatMode)

    property bool isShuffle
    property int playMode
    property int scrollRightMargin: 4

    ScreenTitleArea {
        id: titleArea
        z: 11
        titleText: qsTr("030000") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    SCmnScrollIndicator {
        anchors { top: flickableArea.top; bottom: flickableArea.bottom;
                  right: flickableArea.right; rightMargin: scrollRightMargin; }
        z: 10
        flickable: flickableArea
        visible: (settingItem.height > flickableArea.height)
    }

    Flickable {
        id: flickableArea
        y: titleArea.height
        width: 480
        height: (flickableAreaMaxHeight <= settingItem.height) ? flickableAreaMaxHeight : settingItem.height
        contentHeight: settingItem.height
        boundsBehavior: Flickable.StopAtBounds

        SettingItemCategory {
            id: settingItem

            title: qsTr("020140") + viewstyle.empty_string

            // シャッフル再生
            SettingItemCheckbox {
                title: qsTr("020141") + viewstyle.empty_string
                checked: isShuffle
                onClicked: shuffleOnOffToggled()
            }

            // リピートオフ
            SettingItemRadio {
                title: qsTr("020142") + viewstyle.empty_string
                checked: (playMode === 0)
                exclusiveGroup: playModeGroup
                onClicked: setRepeatMode(0)
            }

            //全曲リピート
            SettingItemRadio {
                title: qsTr("020144") + viewstyle.empty_string
                checked: (playMode === 2)
                exclusiveGroup: playModeGroup
                onClicked: setRepeatMode(2)
            }

            // 1曲リピート
            SettingItemRadio {
                title: qsTr("020143") + viewstyle.empty_string
                checked: (playMode === 1)
                exclusiveGroup: playModeGroup
                onClicked: setRepeatMode(1)
            }

            ExclusiveGroup {
                id: playModeGroup
            }
        }
    }
}

//
// Copyright 2015-2017 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"
import "qrc:/"

Rectangle {
    color: viewstyle.bgcolor.D1

    //! 公開プロパティ
    /** type:var 設定一覧 */
    property var setting_list

    /**
     * 設定項目タップ時シグナル
     * @param value 設定値
     */
    signal selectWirelessQualitySetting(int value)

    /**
     * タイトル
     */
    ScreenTitleArea{
        id: titleArea
        titleText: qsTr("230009") + viewstyle.empty_string
        visibleMusicLibraryTopicon : false
    }

    /**
     * 設定一覧
     */
    ListView {
        id: listView
        anchors {
            top: titleArea.bottom
            bottom: parent.bottom
            left: parent.left
        }
        width: parent.width
        interactive: false
        model: setting_list
        delegate: delegate

        ExclusiveGroup {
            id: listGroup
        }
        Component {
            id: delegate
            SettingItemRadio {
                exclusiveGroup: listGroup
                title: modelData["title"]
                description: modelData["description"]
                checked: modelData["checked"]
                onClicked: {
                    selectWirelessQualitySetting(modelData["value"])
                }
            }
        }
    }
}

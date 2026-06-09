//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/components"
import "qrc:/parts"

Rectangle {
    id: base
    color: viewstyle.bgcolor.D1

    property var listModel
    property string title

    signal select(int value)

    ExclusiveGroup {
        id: settingExclusiveGroup
    }

    ScreenTitleArea {
        id: titleArea
        titleText: title
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    Item {
        id: settingArea
        width: 480
        height: listModel.count * (84 + 1)
        y: titleArea.height
        ListView {
            width: 480
            height: settingArea.height
            interactive: false
            model: listModel
            delegate: settingDelegate
        }
    }

    Component {
        id: settingDelegate
        SettingItemRadio {
            title: label
            checked: is_checked
            exclusiveGroup: settingExclusiveGroup
            onClicked: select(value)
        }
    }
}

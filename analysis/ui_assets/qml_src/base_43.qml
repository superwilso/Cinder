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

    property var keyboardListModel

    signal keyboardChanged(int index)

    ExclusiveGroup {id: keyboardRadioButtonGroup}

    ScreenTitleArea {
        id: titleArea
        titleText: qsTr("990000") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    SettingItemCategory {
        id: category
        y: titleArea.height
        title: qsTr("990007") + viewstyle.empty_string
    }

    ListView {
        id: keyboardSettingList
        y: category.height + category.y
        width: 480
        height: (84 + 1) * 4
        interactive: false
        model: keyboardListModel
        delegate: keyboardSettingListDelegate
        boundsBehavior: Flickable.StopAtBounds
    }
    Component {
        id: keyboardSettingListDelegate
        SettingItemRadio {
            title: label
            checked: isChecked
            exclusiveGroup: keyboardRadioButtonGroup
            onClicked: keyboardChanged(item_id)
        }
    }
}

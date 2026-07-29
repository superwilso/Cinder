//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"

Rectangle {
    id: screenOffTimerSetting
    color: "black"

    property int scrollRightMargin: 4
    property var screenOffTimerList
    signal selectScreenOffTimer(int screenOffTimer)

    ScreenTitleArea {
        id: title
        titleText: qsTr("010026") + viewstyle.empty_string
        fontWeight: Font.Light
    }

    ListView {
        id: list
        width: screenOffTimerSetting.width
        height: screenOffTimerSetting.height - title.height
        y: title.height
        model: screenOffTimerList
        delegate: delegate
        boundsBehavior: Flickable.StopAtBounds
        clip: true
    }

    SCmnScrollIndicator {
        id: scroll
        height: list.height
        anchors {
            top: list.top
            bottom: list.bottom
            right: list.right
            rightMargin: scrollRightMargin
        }
        flickable: list       
    }

    ExclusiveGroup {
        id: radioGroup
    }

    Component {
        id: delegate
        SettingItemRadio {
            title: label
            checked: isChecked
            exclusiveGroup: radioGroup
            onClicked: selectScreenOffTimer(item_id)
        }
    }
}

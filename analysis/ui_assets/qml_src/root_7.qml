//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.0
import QtQuick.Controls 1.1
import QtQuick.Controls.Styles 1.1
import "qrc:/components"
import "qrc:/parts"

Rectangle {
    id: root
    color: viewstyle.bgcolor.D1

    property int dateDisplayFormat
    signal setDateDisplayFormat(int dateDisplayFormat)

    ExclusiveGroup {
        id: group
    }

    ScreenTitleArea {
        id: dateDisplayFormatTitleArea
        z: 11
        titleText: qsTr("010007") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    Column {
        id: column
        y:dateDisplayFormatTitleArea.height

        // YYYY-MM-DD
        SettingItemRadio {
            title: qsTr("010065") + viewstyle.empty_string
            exclusiveGroup: group
            checked: (dateDisplayFormat === 0)
            onClicked: setDateDisplayFormat(0)
        }

        // MM-DD-YYYY
        SettingItemRadio {
            title: qsTr("010066") + viewstyle.empty_string
            exclusiveGroup: group
            checked: (dateDisplayFormat === 1)
            onClicked: setDateDisplayFormat(1)
        }

        // DD-MM-YYYY
        SettingItemRadio {
            title: qsTr("010067") + viewstyle.empty_string
            exclusiveGroup: group
            checked: (dateDisplayFormat === 2)
            onClicked: setDateDisplayFormat(2)
        }
    }
}

//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import QtQuick.Controls 1.2
import "../"
import "qrc:/components"
import "qrc:/parts"

RecorderBaseView {
    property bool isEnableCreateNewFolder

    signal reqCreateFolder()

    ScreenTitleArea {
        id: titleArea
        titleText: qsTr("050003") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }

    // create new folder icon
    SCmnIcon {
        x: 392
        width: 88
        height: 88
        iconPath: "qrc:/assets/images/recorder/btn_direct_rec_create_new_folder_normal.png"
        disabledIconPath: "qrc:/assets/images/recorder/btn_direct_rec_create_new_folder_disable.png"
        pressedIconPath: "qrc:/assets/images/recorder/btn_direct_rec_create_new_folder_pressed.png"
        enabled: isEnableCreateNewFolder
        MouseArea {
            width: parent.width
            height: parent.height
            onClicked: {
                if (parent.recState != "state_start") {
                    reqCreateFolder();
                }
            }
        }
    }
}

//
// Copyright 2015-2016 Sony Corporation
//

import QtQuick 2.3
import "../"
import "qrc:/components"

RecorderBaseView {
    ScreenTitleArea {
        id: titleArea
        titleText: qsTr("050002") + viewstyle.empty_string
        visibleMusicLibraryTopicon: false
        fontWeight: Font.Light
    }
}

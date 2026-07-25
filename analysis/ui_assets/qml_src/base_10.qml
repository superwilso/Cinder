//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3

Rectangle {
    id: base
    signal backButtonClick()
    signal playButtonClick()
    signal libraryButtonClick()
    signal optionButtonClick()
    property bool optionMenuButtonEnabled

    width: 480
    height: 56
    color: "black"
//    border.color: "white"
//    border.width: 1

    FooterButton {
        onBackButtonClick: base.backButtonClick()
        onParentChanged: base.playButtonClick()
        onLibraryButtonClick: base.libraryButtonClick()
        onOptionButtonClick: base.optionButtonClick()
        optionMenuButtonEnabled: base.optionMenuButtonEnabled
    }
}

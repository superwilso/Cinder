//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import QtQuick.Controls 1.2
import "qrc:/parts"
import "qrc:/components"
import "qrc:/framework"

Rectangle {
    id: usbScreen

    anchors.fill:parent
    color: "black"

    property int scrollRightMargin: 4
    property bool usbAutoStatus
    property bool usbConnectStatus
    property bool unmountExported
    property string titleName: qsTr("200015") + viewstyle.empty_string   // "USB接続設定"

    signal usbAutoStatusCliked()
    signal enableExportAsMsc(bool enable)
    signal dialogClose();

    onDialogClose: {
        if(dialog.getMessageID() === "kMid0014") {
            dialog.close()
        }
    }

    ScreenTitleArea {
        id: titleArea
        titleText: titleName
        visibleMusicLibraryTopicon: false
    }

    SCmnScrollIndicator {
        id: scroll
        anchors {
            top: usbScreenSetting.top
            bottom: usbScreenSetting.bottom
            right: usbScreenSetting.right
            rightMargin: scrollRightMargin
        }
        z: 10
        flickable: usbScreenSetting
    }

    Flickable {
        id: usbScreenSetting
        y: titleArea.height
        width: 480
        height: (flickableAreaMaxHeight <= usbScreenSettingItems.height) ? flickableAreaMaxHeight : usbScreenSettingItems.height
        contentHeight: usbScreenSettingItems.height
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: usbScreenSettingItems

            SettingItemCheckbox {
                title: qsTr("200040") + viewstyle.empty_string         // USB自動接続
                description: qsTr("200041") + viewstyle.empty_string   // USB接続時、自動でマスストレージ接続します。
                checked: usbAutoStatus
                onClicked: usbAutoStatusCliked()
            }
            SettingItemNormal {
                id: settingNormal
                enable: usbConnectStatus
                title: qsTr("200018") + viewstyle.empty_string         // USBマスストレージをオンにする
                description: qsTr("200019") + viewstyle.empty_string   // USBマスストレージをオンにするの説明文
                onClicked: {
                    if(unmountExported === false) {
                        dialog.dialogOpen("kMid0014", "", "", 0)
                    } else {
                        exportAsMscChange(false)
                    }
                }
                function exportAsMscChange(enable) {
                    if(true === enable) {
                        enableExportAsMsc(true)
                    } else {
                        enableExportAsMsc(false)
                    }
                }
                DialogWrapper {
                    id: dialog
                    visible: false
                    onAccepted:{
                        settingNormal.exportAsMscChange(true)
                    }
                }
            }
        }
    }
}

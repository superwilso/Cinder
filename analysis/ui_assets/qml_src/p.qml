//
// Copyright 2015-2016 Sony Corporation
//
import QtQuick 2.3
import "qrc:/"
import "qrc:/components"
import "qrc:/parts"

Rectangle {
    property string titleText: ""
    property string titleTextColor: viewstyle.textcolor.L1
    property bool visibleMusicLibraryTopicon
    property bool isLabelHorizontalCenter
    property int fontWeight: Font.Light

    signal musicLibraryTopiconClicked()

    width: 480
    height: 88
    color: viewstyle.bgcolor.D1

    QtObject {
      id: p
      property url icon_path: viewstyle.mode==="reverse"? "qrc:/btn_language_launcher_music_normal.png" : "qrc:/assets/images/btn_launcher_music_normal.png"
      property url pressed_icon_path: viewstyle.mode === "reverse"? "qrc:/btn_language_launcher_music_pressed.png" : "qrc:/assets/images/btn_launcher_music_pressed.png"
      property url disable_icon_path: "qrc:/assets/images/btn_launcher_music_disable.png"
    }

    Row {
        anchors { fill: parent; leftMargin: visibleMusicLibraryTopicon ? 0 : 20; }
        spacing: 10

        // 音楽ライブラリトップアイコン
        SCmnIcon {
          id: icon
            anchors { verticalCenter: parent.verticalCenter }
            width: 88
            height: 88
            visible: visibleMusicLibraryTopicon
            iconPath: p.icon_path
            pressedIconPath: p.pressed_icon_path
            disabledIconPath: p.disable_icon_path
            onClicked: {
                musicLibraryTopiconClicked()
            }
        }

        // タイトル
        SCmnLabel {
            anchors { verticalCenter: parent.verticalCenter
                      horizontalCenter: isLabelHorizontalCenter ? parent.horizontalCenter : undefined }
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            color: titleTextColor
            font.pixelSize: viewstyle.textsize.L
            font.weight: fontWeight
            text: titleText
        }
    }

    // セパレータ
    Image {
      id: main_separator
      anchors { bottom: parent.bottom; left: parent.left; }
      visible: !visibleMusicLibraryTopicon
      source: viewstyle.mode==="reverse"? "qrc:/assets/images/language_study/separator_language_main.png" : "qrc:/assets/images/separator_main.png"
    }
}

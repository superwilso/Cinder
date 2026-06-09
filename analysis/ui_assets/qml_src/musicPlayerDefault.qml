//
// Copyright 2015-2018 Sony Corporation
//
import QtQuick 2.3
import "qrc:/"
import "qrc:/components"

ScreenBase {
    id: musicPlayerDefault

    property real total_playback_time: 0
    property real currently_playing_time: 0
    property int currentry_playtrack_num: 0
    property int total_playtrack_num: 0
    property var entry_id
    property var update_db_counter
    property bool is_activated: false
    /** type:var メタ情報を保持するプロパティ */
    property var meta_data

    signal pressed()
    signal released(int time)
    signal playEnded()

    width: 480
    height: parent.height

    Item {
        id: dialog_p
        property int dialog_index: 0
    }

    QtObject {
        id: p
        property int coverArtDimension: 400
        property int coverArtTop: 36 - 28
    }

    Item {
        id: img
        width: parent.width
        height: p.coverArtTop + p.coverArtDimension
        anchors{top: parent.top; left: parent.left}

        CoverArt {
          width: p.coverArtDimension
          height: p.coverArtDimension
          loading_image_path: "qrc:/ic_audio_play_loading_jacket_picture.png"
          anchors { bottom: img.bottom; horizontalCenter: img.horizontalCenter }
          contents_id: musicPlayerDefault.is_activated? musicPlayerDefault.entry_id : ""
          update_db_counter: musicPlayerDefault.update_db_counter
        }
    }
    PlayerMetaDataArea {
        id: meta
        playTitle: musicPlayerDefault.meta_data["play_title"]
        artistName: musicPlayerDefault.meta_data["artist_name"]
        albumName: musicPlayerDefault.meta_data["album_name"]
        width: musicPlayerDefault.width
        anchors{top: img.bottom; left: musicPlayerDefault.left}
        totalPlaybackTime: musicPlayerDefault.total_playback_time
        currentlyPlayingTime: musicPlayerDefault.currently_playing_time
        modeHiRes: musicPlayerDefault.meta_data["is_high_resolution"]
        lyrics_status: musicPlayerDefault.meta_data["lyrics_status"]
        totalPlayTrackNum: musicPlayerDefault.total_playtrack_num
        currentryPlayTrackNum: musicPlayerDefault.currentry_playtrack_num
        swipeSelector:swipeSelectorTimeScrubBar

        onPressed: musicPlayerDefault.pressed()
        onReleased: musicPlayerDefault.released(time)
        onPlayEnded: musicPlayerDefault.playEnded()
    }
    SwipeSelectorViewJudge{
        id:swipeSelectorTimeScrubBar
        objectName: "swipeSelectorTimeScrubBar"
    }

    function onNotifySeekError()
    {
      meta.resetScrubBarValue()
    }

    Connections {
        target: controller
        onAvailable: {
          viewstyle.mode = "normal"
          meta.isTitleMarquee = true
        }
        onUnavailable: {
          meta.isTitleMarquee = false
        }
    }
}

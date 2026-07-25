//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3
import "qrc:/"
import "qrc:/components"

ScreenBase
{
  id: base

  /** type:var メタ情報を保持するプロパティ */
  property var meta_data

  /** type:var おまかせチャンネルのチャンネルリスト */
  property var sensMeChannelList
  /** type:var おまかせチャンネルの disable になっているチャンネルリスト */
  property var disabledChannelIndexList

  /** type:real 総再生時間 */
  property real totalPlaybackTime
  /** type:real 現在再生時間 */
  property real currentPlayTime

  /** type:int 総トラック数 */
  property int totalPlayTrackNum
  /** type:int 再生中トラック番号 */
  property int currentPlayTrackNum

  /** type:int チャンネル数 */
  property int channelCount
  /** type:int チャンネルアイコンのインデックス */
  property int channelIconIndex

  /** type:int 音声出力先 ID */
  property int output_device_id

  /** type:string Bt Audio 出力時 音質表示文言 */
  property string bt_quality_string
  /** type:bool AVRCP によるボリュームコントロールが有効かどうか */
  property bool is_avrcp_volume_control_available

  /**
   * スクラブバー押下通知
   */
  signal pressed()
  /**
   * スクラブバーリリース通知
   * @param time 時刻
   */
  signal released(int time)
  /**
   * チャンネル選択通知
   */
  signal selectChannelAt(int channelIndex)
  /**
   * チャンネル選択キャンセル通知
   */
  signal cancelSelectChannel()
  /**
   * ボリューム Up 通知
   */
  signal volumeUp()
  /**
   * ボリューム Down 通知
   */
  signal volumeDown()

  onOutput_device_idChanged:
  {
    switch (output_device_id)
    {
    case 3: // USB Audio 接続中
      sound_quality_info.visible = false
      usb_audio_info.visible = true
      bt_audio_info.visible = false
      break;
    case 4: // Bt Audio 接続中
      sound_quality_info.visible = false
      usb_audio_info.visible = false
      bt_audio_info.visible = true
      break;
    default:
      sound_quality_info.visible = true
      usb_audio_info.visible = false
      bt_audio_info.visible = false
      break;
    }
  }

  SoundQualityInfo
  {
    id: sound_quality_info

    anchors { top: base.top; left: base.left; leftMargin: 20 }
    width: 440
    visible: true
    sound_quality_text: meta_data["sound_quality_string"]
    is_high_resolution: meta_data["is_high_resolution"]
  }

  USBAudioInfo
  {
    id: usb_audio_info

    anchors { top: base.top; left: base.left; leftMargin: 100 }
    visible: false
    sound_quality_text: meta_data["sound_quality_string"]
    is_high_resolution: meta_data["is_high_resolution"]
  }

  BtAudioInfo
  {
    id: bt_audio_info

    anchors { top: base.top; left: base.left }
    visible: false
    is_available_avrcp: is_avrcp_volume_control_available
    output_info_text: bt_quality_string

    onVolumeUp:
    {
      base.volumeUp()
    }

    onVolumeDown:
    {
      base.volumeDown()
    }
  }

  Connections
  {
    target: controller
    onAvailable:
    {
       meta.is_title_marquee = true;
       updateChannelIcon();
    }
    onUnavailable:
    {
      meta.is_title_marquee = false;
      if (channel.moving)
      {
        channel.currentIndex = channelIconIndex;
      }
    }
  }

  /**
   * チャンネル情報更新時アイコン切り替え処理
   */
  function onUpdateChannelIconIndex()
  {
    if (!channel.moving)
    {
      updateChannelIcon()
    }
  }

  /**
   * チャンネル切り替え処理
   */
  function updateChannelIcon()
  {
    if (channel.currentIndex !== channelIconIndex)
    {
      channel.currentIndex = channelIconIndex;
    }
  }

  /**
   * シークエラー時処理
   */
  function onNotifySeekError()
  {
    meta.resetScrubBarValue()
  }

  Component
  {
    id: delegate_component

    Row
    {
      property int channelIndex: kChannelIndexRole

      Image
      {
        width: 320
        height: 240
        source: kChannelIconPathRole
        MouseArea
        {
          anchors.fill: parent
          onClicked:
          {
            if (channel.currentIndex != index
             && !channel.moving)
            {
              cancelSelectChannel()
              channel.currentIndex = index
              selectChannelAt(index);
            }
          }
        }
      }
    }
  }

  PathView
  {
    id: channel

    anchors { top: base.top; topMargin: 122; }
    width: 480
    height: 240
    pathItemCount: channelCount < 3 ? channelCount : 3
    preferredHighlightBegin: 0.5
    preferredHighlightEnd: 0.5
    flickDeceleration: 100
    model: sensMeChannelList
    delegate: delegate_component
    path: Path
    {
      startX: -255
      startY: 120
      PathLine
      {
        x: 735
        y: 120
      }
    }

    onMovementStarted:
    {
      cancelSelectChannel()
    }

    onMovementEnded:
    {
      selectChannelAt(channel.currentIndex);
    }
  }

  SensMeChannelPageControl
  {
    anchors { top: channel.bottom; topMargin: 36; left: base.left; leftMargin: 114 }
    width: 252
    height: 18
    nowIndex: channel.currentItem.channelIndex
    disabledIndexList: disabledChannelIndexList
  }

  PlayerMetaDataAreaForSimpleMode
  {
    id: meta

    anchors.bottom: base.bottom
    properties: meta_data
    total_playback_time: base.totalPlaybackTime
    currently_playing_time: base.currentPlayTime
    total_playtrack_num: base.totalPlayTrackNum
    currentry_playtrack_num: base.currentPlayTrackNum
    swipe_selector: swipe_selector_scrub_bar
    is_visible_shuffle_repeat: false

    onPressed: base.pressed()
    onReleased: base.released(time)
  }

  SwipeSelectorViewJudge
  {
    id: swipe_selector_scrub_bar
    objectName: "swipeSelectorTimeScrubBar"
  }

  SwipeSelector
  {
    anchors.fill: parent
    objectName: "swipeSelector"
  }
}

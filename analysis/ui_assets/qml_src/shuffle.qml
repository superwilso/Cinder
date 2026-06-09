//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3
import "qrc:/components"

SwipeDetectionButton
{
  id: shuffle

  /** type:bool シャッフルの状態 */
  property bool is_shuffle: false
  /** type:bool シンプルモードか否か */
  property bool is_simple_mode: false
  /** type:string 通常時画像 */
  property string load_image_source: ""
  /** type:string 押下時画像 */
  property string pressed_image_source: ""

  /**
   * スワイプ開始検知
   */
  signal startedSwipe()

  width: 65
  height: 110
  loadComponent: Image { source: load_image_source }
  pressedComponent: Image { source: pressed_image_source }
  // ShuffleButton が disable は透明を意味するのだが、
  // 透明画像をそのまま割り当ててしまうと、 FadeIn / FadeOut のアニメーションをすることができない
  // そのため、通常時の画像を表示とする
  disableComponent: Image { source: load_image_source }

  onIs_shuffleChanged:
  {
    changeImage()
  }

  onIs_simple_modeChanged:
  {
    changeImage()
  }

  onSwipeDetect:
  {
    shuffle.startedSwipe()
  }

  /**
   * 画像を設定する
   */
  function changeImage()
  {
    if (is_simple_mode && is_shuffle)
    {
      load_image_source = "qrc:/assets/images/btn_audio_play_hp_amp_shuffle_on_normal.png"
      pressed_image_source = "qrc:/assets/images/btn_audio_play_hp_amp_shuffle_on_pressed.png"
    }
    else if (is_simple_mode && !is_shuffle)
    {
      load_image_source = "qrc:/assets/images/btn_audio_play_hp_amp_shuffle_off_normal.png"
      pressed_image_source = "qrc:/assets/images/btn_audio_play_hp_amp_shuffle_off_pressed.png"
    }
    else if (!is_simple_mode && is_shuffle)
    {
      load_image_source = "qrc:/assets/images/btn_audio_play_shuffle_on_normal.png"
      pressed_image_source = "qrc:/assets/images/btn_audio_play_shuffle_on_pressed.png"
    }
    else if (!is_simple_mode && !is_shuffle)
    {
      load_image_source = "qrc:/assets/images/btn_audio_play_shuffle_off_normal.png"
      pressed_image_source = "qrc:/assets/images/btn_audio_play_shuffle_off_pressed.png"
    }
    else
    {
      // 実際には到達しない
      load_image_source = ""
      pressed_image_source = ""
    }
  }

  Component.onCompleted:
  {
    changeImage()
  }
}

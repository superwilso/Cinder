//
// Copyright 2018 Sony Corporation
//

import QtQuick 2.3

Rectangle
{
  id: base

  /** type:bool ブラインド中かどうか(ブラインドしている最中も含む) */
  readonly property bool is_bliding: visible

  color: viewstyle.bgcolor.D1
  opacity: 0
  visible: false

  /**
   * シンプルモード表示処理を行う
   * @param is_blind true: シンプルモード表示 / false: 通常表示
   */
  function changeBlindStatus(is_blind)
  {
    if (is_blind)
    {
      state = "FadeOut"
    }
    else
    {
      state = "FadeIn"
    }
  }

  /**
   * アニメーションなしシンプルモード表示処理を行う
   */
  function forceBlind()
  {
    opacity = 1
    state = "FadeOut"
  }

  states:
  [
    State
    {
      name: "FadeOut"
      PropertyChanges
      {
        target: base
        opacity: 1
      }
    },
    State
    {
      name: "FadeIn"
      PropertyChanges
      {
        target: base
        opacity: 0
      }
    }
  ]

  transitions:
  [
    Transition
    {
      to: "FadeOut"

      SequentialAnimation
      {
        PropertyAction
        {
          targets: base
          property: "visible"
          value: true
        }
        NumberAnimation
        {
          targets: base
          property: "opacity"
          duration: 400
          easing.type: Easing.Linear
          to: 1
        }
      }
    },
    Transition
    {
      to: "FadeIn"

      SequentialAnimation
      {
        NumberAnimation
        {
          targets: base
          property: "opacity"
          duration: 50
          easing.type: Easing.Linear
          to: 0
        }
        PropertyAction
        {
          targets: base
          property: "visible"
          value: false
        }
      }
    }
  ]
}

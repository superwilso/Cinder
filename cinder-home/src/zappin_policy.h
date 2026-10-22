#pragma once

// Small deterministic decisions shared by the production poll path and its host harness.
inline bool zappin_seek_was_sent(int guard_result, int seek_result) {
    return guard_result == 0 && seek_result == 0;
}

inline bool zappin_resume_after_confirmation(bool user_paused) {
    return !user_paused;
}

// A URI snapshot can arrive late and identify the previous occurrence of the same URI.
// PlayerService starts each newly submitted preview sequence at zero before the shell seeks to
// its planned SABI/fallback offset, so require that reset position as well as the URI match.
inline bool zappin_new_request_position_ready(bool have_position, int current_ms) {
    return have_position && current_ms >= 0 && current_ms <= 1500;
}

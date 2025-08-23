#pragma once

typedef enum {
    BLACK = 30,
    RED,
    GREEN,
    YELLOW,
    BLUE,
    MAGENTA,
    CYAN,
    WHITE,
    BRIGHT_BLACK = 90,
    BRIGHT_RED,
    BRIGHT_GREEN,
    BRIGHT_YELLOW,
    BRIGHT_BLUE,
    BRIGHT_MAGENT,
    BRIGHT_CYAN,
    BRIGHT_WHITE,
} color_t;

typedef enum {
    ERROR,
    WARN,
    INFO,
    DEBUG,
    TRACE,
} LogLevelType;

void Log(LogLevelType level, const char* fmt, ...);

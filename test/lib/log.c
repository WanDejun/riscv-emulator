#include "log.h"
#include "io.h"
#include <stdarg.h>

void Log(LogLevelType level, const char *fmt, ...) {
  const char *tag;
  int color;

  switch (level) {
  case ERROR:
    tag = "ERROR";
    color = RED;
    break;
  case WARN:
    tag = "WARN";
    color = YELLOW;
    break;
  case INFO:
    tag = "INFO";
    color = BLUE;
    break;
  case DEBUG:
    tag = "DEBUG";
    color = GREEN;
    break;
  case TRACE:
    tag = "TRACE";
    color = BRIGHT_BLACK;
    break;
  default:
    return;
  }

  va_list ap;
  va_start(ap, fmt);

  va_list args;
  va_copy(args, ap);

  // printf("\x1b[%dm[%s] ", color, tag);
  printf("[%s] ", tag);
  vprintf(fmt, args);
  printf("\n");
  // printf("\x1b[0m\n");

  va_end(args);
  va_end(ap);
}
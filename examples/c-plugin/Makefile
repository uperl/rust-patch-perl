# Build the C example plugin.
#
#   make            # build lib<NAME>.<ext> for this platform
#   make clean

NAME    := patch_perl_plugin_cexample
CC      ?= cc
CFLAGS  ?= -O2 -Wall -Wextra -fPIC
INCLUDE := -I../../include

UNAME_S := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
  EXT     := bundle
  LDFLAGS := -bundle -undefined dynamic_lookup
else
  EXT     := so
  LDFLAGS := -shared
endif

TARGET := lib$(NAME).$(EXT)

$(TARGET): plugin.c ../../include/patch_perl_plugin.h
	$(CC) $(CFLAGS) $(INCLUDE) $(LDFLAGS) -o $@ plugin.c

.PHONY: clean
clean:
	rm -f lib$(NAME).so lib$(NAME).dylib lib$(NAME).bundle $(NAME).dll

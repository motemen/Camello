#!/usr/bin/perl
# Test various delimiters for q() expressions

# Common delimiters that should work
print q(hello);       # parentheses
print q/world/;       # slash
print q{foo};         # braces
print q[bar];         # brackets
print q<baz>;         # angle brackets

# Less common but valid delimiters
print q|pipe|;        # pipe
print q#hash#;        # hash
print q@at@;          # at sign
print q%percent%;     # percent
print q^caret^;       # caret
print q*asterisk*;    # asterisk
print q+plus+;        # plus
print q=equals=;      # equals
print q!exclamation!; # exclamation
print q~tilde~;       # tilde
print q`backtick`;    # backtick
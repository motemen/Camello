# For loop with when/default clauses
for my $item (@items) {
    when ($item > 10) {
        say "Large: $item";
    }
    when ($item < 0) {
        say "Negative: $item";
    }
    default {
        say "Normal: $item";
    }
}

# For loop with implicit $_ and when clauses
for (@values) {
    when (/^test/) {
        say "Test value: $_";
    }
    when ($_ == 42) {
        say "The answer!";
    }
    default {
        say "Other: $_";
    }
}

# C-style for with when/default
for (my $i = 0; $i < 10; $i++) {
    when ($i % 2 == 0) {
        say "Even: $i";
    }
    default {
        say "Odd: $i";
    }
}

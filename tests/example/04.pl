sub foo {
    my sub bar {
    }

    warn <<~"TEXT"
        Hello,
        World!
        TEXT
}

unless (1) {
    1 while 1;
}

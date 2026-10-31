# The journal loop: play a journaled drive, then fold it back and settle up.
Output docs/assets/cli-recover.gif
Set FontSize 15
Set Width 940
Set Height 760
Set Shell bash
Set CursorBlink false
Set TypingSpeed 5ms
Type "playbook mvp --journal /tmp/peeve-drive.jsonl"
Enter
Sleep 7s
Type "clear && playbook recover /tmp/peeve-drive.jsonl"
Enter
Sleep 6s
Type "clear && playbook recover /tmp/no-such-journal.jsonl"
Enter
Sleep 4s

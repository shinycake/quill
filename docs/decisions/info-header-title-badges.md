# Info panel title and header badges

tdesktop (`info/profile/info_profile_widget.cpp:344-351`) titles the panel
"User Info", "Bot Info", "Group Info" or "Channel Info" by peer type, and
(`info_profile_badge.cpp:97-99`) shows the verified, Scam, Fake and emoji
status badges after the name.

Quill used "Contact info" for every user and "Group Info" for every
supergroup, and the supergroup panel drew only the bot-verification icon.

Now: the title follows the peer type (new lang keys `lng_info_user_title`,
`lng_info_bot_title`, `lng_info_channel_title`), and the group/channel header
draws `Session::chat_header_badges` through the same `title_badge_element` the
user panel and chat header use. Saved Messages and topic panels are not
routed through this panel.

Verified with the `ready-info-channel` demo in light and dark.

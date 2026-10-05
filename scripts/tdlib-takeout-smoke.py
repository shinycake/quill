#!/usr/bin/env python3
"""Offline check of the compiled Quill TDLib extension; no client or account."""
import ctypes
import json
import sys

lib = ctypes.CDLL(sys.argv[1])
lib.td_execute.argtypes = [ctypes.c_char_p]
lib.td_execute.restype = ctypes.c_char_p
lib.td_execute(b'{"@type":"setLogVerbosityLevel","new_verbosity_level":0}')

def execute(method):
    return json.loads(lib.td_execute(json.dumps({"@type": method}).encode()))

unknown = execute("quillIntentionallyUnknownMethod")
assert unknown["@type"] == "error" and "Unknown class" in unknown["message"], unknown
for method in ["getQuillSavedContacts", "getQuillTakeoutMessageRanges"]:
    known = execute(method)
    assert known["@type"] == "error" and "synchronously" in known["message"], (method, known)
print("PASS: compiled contact and message-range requests recognized; account and network unused")

import gdb

class Register16Printer:
    def __init__(self, val):
        self.val = val

    def to_string(self):
        return hex(self.val["__0"])

class CpuFlagsPrinter:
    def __init__(self, val):
        self.val = val

    def to_string(self):
        return "Hi :3"

def lookup(val):
    lookup_tag = val.type.tag
    if lookup_tag is None:
        return None
    if "snes_emu::cpu::Register16" == lookup_tag:
        return Register16Printer(val)
    return None

gdb.current_objfile().pretty_printers.append(lookup)

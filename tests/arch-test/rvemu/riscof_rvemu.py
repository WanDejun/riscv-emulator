import os
import re
import shutil
import subprocess
import shlex
import logging
import random
import string
from string import Template
import sys

import riscof.utils as utils
from riscof.pluginTemplate import pluginTemplate

logger = logging.getLogger()


class rvemu(pluginTemplate):
    __model__ = "rvemu"

    # TODO: please update the below to indicate family, version, etc of your DUT.
    __version__ = "XXX"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        config = kwargs.get("config")

        # If the config node for this DUT is missing or empty. Raise an error. At minimum we need
        # the paths to the ispec and pspec files
        if config is None:
            print("Please enter input file paths in configuration.")
            raise SystemExit(1)

        # In case of an RTL based DUT, this would be point to the final binary executable of your
        # test-bench produced by a simulator (like verilator, vcs, incisive, etc). In case of an iss or
        # emulator, this variable could point to where the iss binary is located. If 'PATH variable
        # is missing in the config.ini we can hardcode the alternate here.

        if "PATH" in config:
            self.dut_exe = (
                config["PATH"]
                if os.path.isabs(config["PATH"])
                else os.path.join(os.getcwd(), config["PATH"], "riscv-emulator")
            )

        # Number of parallel jobs that can be spawned off by RISCOF
        # for various actions performed in later functions, specifically to run the tests in
        # parallel on the DUT executable. Can also be used in the build function if required.
        self.num_jobs = str(config["jobs"] if "jobs" in config else 1)

        # Path to the directory where this python file is located. Collect it from the config.ini
        self.pluginpath = os.path.abspath(config["pluginpath"])

        # Collect the paths to the  riscv-config based ISA and platform yaml files. One can choose
        # to hardcode these here itself instead of picking it from the config.ini file.
        self.isa_spec = os.path.abspath(config["ispec"])
        self.platform_spec = os.path.abspath(config["pspec"])

        # We capture if the user would like the run the tests on the target or
        # not. If you are interested in just compiling the tests and not running
        # them on the target, then following variable should be set to False
        if "target_run" in config and config["target_run"] == "0":
            self.target_run = False
        else:
            self.target_run = True

    def initialise(self, suite, work_dir, archtest_env):

        # capture the working directory. Any artifacts that the DUT creates should be placed in this
        # directory. Other artifacts from the framework and the Reference plugin will also be placed
        # here itself.
        self.work_dir = work_dir

        # capture the architectural test-suite directory.
        self.suite_dir = suite

        # Note the march is not hardwired here, because it will change for each
        # test. Similarly the output elf name and compile macros will be assigned later in the
        # runTests function
        self.compile_cmd = (
            "riscv{1}-unknown-elf-gcc -march={0} \
         -static -mcmodel=medany -fvisibility=hidden -nostdlib -nostartfiles -g\
         -T "
            + self.pluginpath
            + "/env/link.ld\
         -I "
            + self.pluginpath
            + "/env/\
         -I "
            + archtest_env
            + " {2} -o {3} {4}"
        )

        # add more utility snippets here

    def build(self, isa_yaml, platform_yaml):

        # load the isa yaml as a dictionary in python.
        ispec = utils.load_yaml(isa_yaml)["hart0"]

        # capture the XLEN value by picking the max value in 'supported_xlen' field of isa yaml. This
        # will be useful in setting integer value in the compiler string (if not already hardcoded);
        self.xlen = "64" if 64 in ispec["supported_xlen"] else "32"

        # for spike start building the '--isa' argument. the self.isa is dutnmae specific and may not be
        # useful for all DUTs
        isa_str = ispec["ISA"]
        isa_str = isa_str.replace("RV", "")  # remove RV prefix if present

        self.isa = "rv" + self.xlen

        # XXX: This is from the original spike plugin. Why we don't just use the isa_str directly?
        if "I" in isa_str:
            self.isa += "i"
        if "E" in isa_str:
            self.isa += "e"
        if "M" in isa_str:
            self.isa += "m"
        if "A" in isa_str:
            self.isa += "a"
        if "F" in isa_str:
            self.isa += "f"
        if "D" in isa_str:
            self.isa += "d"
        if "Q" in isa_str:
            self.isa += "q"
        if "C" in isa_str:
            self.isa += "c"
        if "V" in isa_str:
            self.isa += "v"
        if "Zifencei" in isa_str:
            self.isa += "_Zifencei"
        if "Zicsr" in isa_str:
            self.isa += "_Zicsr"
        if "Zicond" in isa_str:
            self.isa += "_Zicond"
        if "Zicboz" in isa_str:
            self.isa += "_Zicboz"
        if "Zimop" in isa_str:
            self.isa += "_Zimop"
        if "Zfa" in isa_str:
            self.isa += "_Zfa"
        if "Zfh" in isa_str:
            self.isa += "_Zfh"
        if "Zca" in isa_str:
            self.isa += "_Zca"
        if "Zcb" in isa_str:
            self.isa += "_Zcb"
        if "Zcmop" in isa_str:
            self.isa += "_Zcmop"
        if "Zba" in isa_str:
            self.isa += "_Zba"
        if "Zbb" in isa_str:
            self.isa += "_Zbb"
        if "Zbc" in isa_str:
            self.isa += "_Zbc"
        if "Zbs" in isa_str:
            self.isa += "_Zbs"
        if "Zbkb" in isa_str:
            self.isa += "_Zbkb"
        if "Zbkc" in isa_str:
            self.isa += "_Zbkc"
        if "Zknd" in isa_str:
            self.isa += "_Zknd"
        if "Zkne" in isa_str:
            self.isa += "_Zkne"
        if "Zbkx" in isa_str:
            self.isa += "_Zbkx"
        if "Zknh" in isa_str:
            self.isa += "_Zknh"
        if "Zksh" in isa_str:
            self.isa += "_Zksh"
        if "Zksed" in isa_str:
            self.isa += "_Zksed"

        # TODO: The following assumes you are using the riscv-gcc toolchain. If
        #      not please change appropriately
        self.compile_cmd = (
            self.compile_cmd
            + " -mabi="
            + (
                "lp64 "
                if 64 in ispec["supported_xlen"]
                else ("ilp32e " if "E" in isa_str else "ilp32 ")
            )
        )

        if "pmp-grain" in ispec["PMP"]:
            # if the PMP granularity is specified in the isa yaml, then we use that value
            # convert from G to bytes: g = 2^(G+2) bytes
            self.granularity = pow(2, ispec["PMP"]["pmp-grain"] + 2)
        else:
            self.granularity = 4  # default granularity is 4 bytes

    def runTests(self, testList):

        # Delete Makefile if it already exists.
        if os.path.exists(self.work_dir + "/Makefile." + self.name[:-1]):
            os.remove(self.work_dir + "/Makefile." + self.name[:-1])
        # create an instance the makeUtil class that we will use to create targets.
        make = utils.makeUtil(
            makefilePath=os.path.join(self.work_dir, "Makefile." + self.name[:-1])
        )

        # set the make command that will be used. The num_jobs parameter was set in the __init__
        # function earlier
        make.makeCommand = "make -k -j" + self.num_jobs

        # we will iterate over each entry in the testList. Each entry node will be referred to by the
        # variable testname.
        for testname in testList:

            # for each testname we get all its fields (as described by the testList format)
            testentry = testList[testname]

            # we capture the path to the assembly file of this test
            test = testentry["test_path"]

            # capture the directory where the artifacts of this test will be dumped/created. RISCOF is
            # going to look into this directory for the signature files
            test_dir = testentry["work_dir"]

            # name of the elf file after compilation of the test
            elf = "my.elf"

            # name of the signature file as per requirement of RISCOF. RISCOF expects the signature to
            # be named as DUT-<dut-name>.signature. The below variable creates an absolute path of
            # signature file.
            sig_file = os.path.join(test_dir, self.name[:-1] + ".signature")
            log_file = os.path.join(test_dir, self.name[:-1] + ".log")

            # for each test there are specific compile macros that need to be enabled. The macros in
            # the testList node only contain the macros/values. For the gcc toolchain we need to
            # prefix with "-D". The following does precisely that.
            compile_macros = " -D" + " -D".join(testentry["macros"])

            # substitute all variables in the compile command that we created in the initialize
            # function
            #          cmd = self.compile_cmd.format(testentry['isa'].lower().replace('zicsr', ' ', 2), self.xlen, test, elf, compile_macros)
            cmd = self.compile_cmd.format(
                self.isa.lower(), self.xlen, test, elf, compile_macros
            )

            # if the user wants to disable running the tests and only compile the tests, then
            # the "else" clause is executed below assigning the sim command to simple no action
            # echo statement.
            if self.target_run:
                # set up the simulation command. Template is for spike. Please change.
                # simcmd = self.dut_exe + ' --misaligned --isa={0} +signature={1} +signature-granularity=8 {2}'.format(self.isa, sig_file, elf)
                # simcmd = simcmd + ';' + self.dut_exe + ' --isa={0} --log-commits -l my.elf 2> {1}'.format(self.isa, log_file)

                simcmd = "{} {} --signature {} --signature-granularity 8 --max-cycles 1000000 --loglevel info > {} 2>&1".format(
                    self.dut_exe, elf, sig_file, log_file
                )
            else:
                simcmd = 'echo "NO RUN"'

            # concatenate all commands that need to be executed within a make-target.
            execute = "@cd {}; {}; {};".format(testentry["work_dir"], cmd, simcmd)

            # create a target. The makeutil will create a target with the name "TARGET<num>" where num
            # starts from 0 and increments automatically for each new target that is added
            make.add_target(execute)

        # if you would like to exit the framework once the makefile generation is complete uncomment the
        # following line. Note this will prevent any signature checking or report generation.
        # raise SystemExit

        # once the make-targets are done and the makefile has been created, run all the targets in
        # parallel using the make command set above.
        # make.execute_all(self.work_dir)
        # DH 7/26/22 increase timeout to 1800 seconds so sim will finish on slow machines
        # DH 5/17/23 increase timeout to 3600 seconds
        make.execute_all(self.work_dir, timeout=3600)

        # if target runs are not required then we simply exit as this point after running all
        # the makefile targets.
        if not self.target_run:
            raise SystemExit(0)

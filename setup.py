#! /usr/bin/env python3

from distutils.core import setup

setup(
	name="dura",
	version="0.0.0",
	description="dura: a robust backup system",
	author="Martin Pool",
	author_email="mbp+dura@sourcefrog.net",
	# url="http://dura.sourcefrog.net/",
	packages=["duralib"],
	scripts=["dura"],
	classifiers=[
		"Development Status :: 2 - Pre-Alpha",
		"Topic :: System :: Archiving :: Backup",
		"License :: OSI Approved :: Apache Software License",
	],
	)
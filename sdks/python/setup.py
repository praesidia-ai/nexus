"""Setup for the Nexus AI Python SDK."""
from setuptools import setup, find_packages

setup(
    name="nexus-sdk",
    version="0.1.0",
    description="Python SDK for the Nexus AI agent platform",
    packages=find_packages(),
    python_requires=">=3.9",
    install_requires=[],
    extras_require={
        "async": ["aiohttp>=3.9"],
    },
)

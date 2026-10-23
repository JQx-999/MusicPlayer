# Contributing to this project

First off, I really thank you that you consider contributing to this project! It really means a lot and it is because of people like you who make open source such an amazing place to learn, create, experiment and explore. A little by little efforts from people like you create massive changes that delivers value to this world. 

---

## Code  of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please report unacceptable behavior to the project maintainers.

---

## Some things to know

- Test the fix/feature yourself.
- Use of AI assistance in developing quality feature, updates and fixes or do a review of the code is **fully accepted and encouraged**. It can significantly increase efficiency with a good accuracy when used thoughtfully.

    However, to maintain project quality,
    - You are legally and functionally responsible for all the code you submit. Please make sure you thoroughly test and understand your changes before opening a PR.
    - Ensure AI-generated code doesn't introduce security vulnerabilities, dead code, or broken dependencies.
    - Please do not submit raw, unverified AI outputs without testing them locally first.
    - If an AI significantly helped structure your PR, feel free to mention it in the Pull Request description.
    - You should be able to answer questions about your PR during code review. If the answer to a maintainer question is "I don't know, the AI wrote it," the PR may be closed.

---

## How to Contribute?

Here are the clear steps provided to contribute in this project.

### 1. Reporting Bugs
Before creating a bug report, please check existing issues if it has already been reported. When submitting a bug report, please include:
- A clear, brief title.
- Steps on how to reproduce the bug.
- Expected vs. actual behaviour.
- Environment details (OS, GLIBC version, runtime versions).

### 2. Suggesting Enhancements
Feature requests are tracked as GitHub issues. Please provide:
- A clear and brief title.
- A step-by-step explanation of the suggested feature.
- Examples of mockups if available.

---

## Development Workflow

Follow these steps to propose changes.

1. ### Setup the OS

    Currently, primary development is tested on **Linux**.
    
    **For Linux:**

    - Use a linux distro, preferrably Debian, of your choice or setup a virtual machine to use it as a development environment.

    - **Install the dependencies:**

        For Debian/Ubuntu based:

        ```bash
        sudo apt install libglib2.0-dev libgtk-3-dev pkg-config build-essential libwebkit2gtk-4.1-dev libasound2-dev ffmpeg librsvg2-2 librsvg2-bin clang libavcodec-dev libavdevice-dev libavfilter-dev libavformat-dev libavutil-dev libswresample-dev libswscale-dev
        ```

        For Fedora/RHEL based:

        ```bash
        sudo dnf groupinstall "Development Tools" && sudo dnf install glib2-devel gtk3-devel pkgconf-pkg-config webkit2gtk4.1-devel alsa-lib-devel ffmpeg ffmpeg-devel librsvg2 librsvg2-tools clang
        ```

        For Arch based:

        ```bash
        sudo pacman -Syu --needed base-devel glib2 gtk3 pkgconf webkit2gtk-4.1 alsa-lib ffmpeg librsvg clang
        ```

    *Note: If you are running macOS or Windows and run into setup issues, PRs updating our setup docs for those platforms are very welcome!*


<br>

2. **Fork the Repository**
    
    Click the **Fork** button at the top right of this page to create a copy on your account.

<br>

3. **Clone your Fork**

    ```bash
   git clone https://github.com/YOUR-USERNAME/your-repo-name.git
   cd your-repo-name
    ```
<br>

4. **Create a Feature Branch**

    Create a branch off `main` with a descriptive name:

    ```bash
    git checkout -b feature/your-feature-name
    # or for bug fixes:
    git checkout -b fix/issue-description
    ```
<br>

5. **Install all required packages**
    
    Run the following command to download all required packages. 

    ```bash
    # install packages
    npm install
    ```

<br>

6. **Make Your Changes & Test**

    Write your code and run tests to ensure existing functionality remains intact:

    ```bash
    npm run tauri dev
    ```

<br>

7. **Commit Your Changes**

    Write clear, concise commit messages:

    ```bash
    git commit -m "feat: add user authentication form validation"
    ```

<br>

8. **Push to GitHub**

    Push your changes to your fork:

    ```bash
    git push origin feature/your-feature-name
    ```

9. **Open a Pull Request (PR)**

    Navigate to the original repository on GitHub. You will see a banner asking to "**Compare & pull request**". Describe your changes and submit it for review!

## Commit Message Conventions

We recommend keeping commit messages clean and standardized:

`feat` : A new feature

`fix` : A bug fix

`docs` : Documentation-only changes

`style` : Formatting changes (spaces, semicolons, etc.)

`refactor` : Code changes that neither fix bugs nor add features

`test` : Adding or updating tests
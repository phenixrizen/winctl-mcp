#!/usr/bin/env bash
set -euo pipefail

manifest_version="$(awk -F\" '/^version = / { print $2; exit }' Cargo.toml)"
branch="${GITHUB_REF_NAME:-$(git rev-parse --abbrev-ref HEAD)}"
run_number="${GITHUB_RUN_NUMBER:-0}"
short_sha="$(git rev-parse --short HEAD)"
force_version="${RELEASE_FORCE_VERSION:-}"

latest_tag="$(
  git tag --merged HEAD --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname \
    | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
    | head -n 1 || true
)"

if [[ -n "${latest_tag}" ]]; then
  base_version="${latest_tag#v}"
  commit_range="${latest_tag}..HEAD"
else
  base_version="${manifest_version}"
  commit_range="HEAD"
fi

commit_count="$(git rev-list --count "${commit_range}" 2>/dev/null || printf '0')"
commit_messages="$(git log --format=%B "${commit_range}" 2>/dev/null || true)"

if [[ -n "${force_version}" ]]; then
  version="${force_version#v}"
  bump="forced"
  should_release="true"
else
  IFS=. read -r major minor patch <<< "${base_version}"

  if [[ -z "${latest_tag}" ]]; then
    bump="initial"
  elif grep -Eq '^BREAKING CHANGE:|^[A-Za-z]+(\([^)]+\))?!:' <<< "${commit_messages}"; then
    major=$((major + 1))
    minor=0
    patch=0
    bump="major"
  elif grep -Eq '^feat(\([^)]+\))?!?:' <<< "${commit_messages}"; then
    minor=$((minor + 1))
    patch=0
    bump="minor"
  elif [[ "${commit_count}" -gt 0 ]]; then
    patch=$((patch + 1))
    bump="patch"
  else
    bump="none"
  fi

  next_version="${major}.${minor}.${patch}"

  case "${branch}" in
    main)
      version="${next_version}"
      ;;
    beta)
      version="${next_version}-beta.${run_number}"
      ;;
    alpha)
      version="${next_version}-alpha.${run_number}"
      ;;
    *)
      safe_branch="$(printf '%s' "${branch}" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^0-9a-z]+/-/g; s/^-+//; s/-+$//')"
      if [[ -z "${safe_branch}" ]]; then
        safe_branch="dev"
      fi
      version="${next_version}-${safe_branch}.${short_sha}"
      ;;
  esac

  if [[ "${bump}" == "none" ]]; then
    should_release="false"
  else
    should_release="true"
  fi
fi

if [[ "${version}" == *"-"* ]]; then
  prerelease="true"
else
  prerelease="false"
fi

tag="v${version}"

printf 'VERSION=%s\n' "${version}"
printf 'TAG=%s\n' "${tag}"
printf 'BASE_VERSION=%s\n' "${base_version}"
printf 'BUMP=%s\n' "${bump}"
printf 'PRERELEASE=%s\n' "${prerelease}"
printf 'SHOULD_RELEASE=%s\n' "${should_release}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    printf 'version=%s\n' "${version}"
    printf 'tag=%s\n' "${tag}"
    printf 'base_version=%s\n' "${base_version}"
    printf 'bump=%s\n' "${bump}"
    printf 'prerelease=%s\n' "${prerelease}"
    printf 'should_release=%s\n' "${should_release}"
  } >> "${GITHUB_OUTPUT}"
fi

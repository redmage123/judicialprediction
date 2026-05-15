"""
Management command: mint_pat — S6.15.

Mints a new Personal Access Token for an existing Operator and prints the
plaintext exactly once.  The Operator can then use the plaintext as an
``Authorization: Bearer pat_*`` header against the public REST API.

Usage
-----
    uv run python manage.py mint_pat --email braun@example.test --name "CI integration"

The plaintext is printed to STDOUT.  After this command exits there is no
way to recover it — only the SHA-256 hash is persisted.
"""

from __future__ import annotations

from django.core.management.base import BaseCommand, CommandError

from operators.models import Operator
from operators.pat_models import PersonalAccessToken


class Command(BaseCommand):
    help = "Mint a new PAT for an existing operator (S6.15)."

    def add_arguments(self, parser) -> None:
        parser.add_argument(
            "--email",
            required=True,
            help="Operator email; matched case-insensitively.",
        )
        parser.add_argument(
            "--name",
            required=True,
            help="Human-readable label for this token (shown in admin lists).",
        )

    def handle(self, *args, **options) -> None:
        email: str = options["email"]
        name: str = options["name"]

        try:
            operator = Operator.objects.get(email__iexact=email)
        except Operator.DoesNotExist as exc:
            raise CommandError(f"no operator with email {email!r}") from exc

        if operator.role == "super":
            # super operators have tenant_id NULL — PAT-auth needs a
            # tenant to bind to, so refuse to mint here.
            raise CommandError(
                "PATs cannot be minted for role='super' operators (no tenant scope)"
            )

        instance, plaintext = PersonalAccessToken.objects.mint(operator, name)

        self.stdout.write(self.style.SUCCESS(f"PAT id: {instance.id}"))
        self.stdout.write(self.style.SUCCESS(f"PAT name: {name}"))
        self.stdout.write(self.style.SUCCESS(f"Operator: {operator.email} (tenant {operator.tenant_id})"))
        self.stdout.write("")
        self.stdout.write(self.style.WARNING("PLAINTEXT (save this now — not shown again):"))
        self.stdout.write(plaintext)
